use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::blocking::Client;
use reqwest::Url;
use serde::Serialize;
use serde_json::Value;

mod account;
mod idp;
pub mod token;

use crate::app::AppError;
use crate::app::FirebaseApp;
use crate::auth::error::{AuthError, AuthResult};
use crate::auth::model::{
    AuthConfig, AuthCredential, AuthStateListeners, EmailAuthProvider, SignInWithPasswordRequest,
    SignInWithPasswordResponse, SignUpRequest, SignUpResponse, User, UserCredential, UserInfo,
};
use crate::auth::oauth::{
    credential::OAuthCredential, InMemoryRedirectPersistence, OAuthPopupHandler,
    OAuthRedirectHandler, PendingRedirectEvent, RedirectOperation, RedirectPersistence,
};
use crate::auth::persistence::{
    AuthPersistence, InMemoryPersistence, PersistedAuthState, PersistenceListener,
    PersistenceSubscription,
};
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentContainer, ComponentType};
use crate::firestore::remote::datastore::TokenProviderArc;
use crate::util::{backoff, PartialObserver};
use account::{
    confirm_password_reset, delete_account, send_email_verification, send_password_reset_email,
    update_account, verify_password, UpdateAccountRequest, UpdateAccountResponse, UpdateString,
};
use idp::{sign_in_with_idp, SignInWithIdpRequest, SignInWithIdpResponse};

const DEFAULT_OAUTH_REQUEST_URI: &str = "http://localhost";

pub struct Auth {
    app: FirebaseApp,
    config: AuthConfig,
    current_user: Mutex<Option<Arc<User>>>,
    listeners: AuthStateListeners,
    rest_client: Client,
    token_refresh_tolerance: Duration,
    persistence: Arc<dyn AuthPersistence + Send + Sync>,
    persisted_state_cache: Mutex<Option<PersistedAuthState>>,
    persistence_subscription: Mutex<Option<PersistenceSubscription>>,
    popup_handler: Mutex<Option<Arc<dyn OAuthPopupHandler>>>,
    redirect_handler: Mutex<Option<Arc<dyn OAuthRedirectHandler>>>,
    redirect_persistence: Mutex<Arc<dyn RedirectPersistence>>,
    oauth_request_uri: Mutex<String>,
    refresh_cancel: Mutex<Option<Arc<AtomicBool>>>,
    self_ref: Mutex<Weak<Auth>>,
}

impl Auth {
    pub fn builder(app: FirebaseApp) -> AuthBuilder {
        AuthBuilder::new(app)
    }

    pub fn new(app: FirebaseApp) -> AuthResult<Self> {
        Self::new_with_persistence(app, Arc::new(InMemoryPersistence::default()))
    }

    pub fn new_with_persistence(
        app: FirebaseApp,
        persistence: Arc<dyn AuthPersistence + Send + Sync>,
    ) -> AuthResult<Self> {
        let api_key = app
            .options()
            .api_key
            .clone()
            .ok_or_else(|| AuthError::InvalidCredential("Missing API key".into()))?;

        let config = AuthConfig {
            api_key: Some(api_key),
        };

        Ok(Self {
            app,
            config,
            current_user: Mutex::new(None),
            listeners: AuthStateListeners::default(),
            rest_client: Client::new(),
            token_refresh_tolerance: Duration::from_secs(5 * 60),
            persistence,
            persisted_state_cache: Mutex::new(None),
            persistence_subscription: Mutex::new(None),
            popup_handler: Mutex::new(None),
            redirect_handler: Mutex::new(None),
            redirect_persistence: Mutex::new(InMemoryRedirectPersistence::shared()),
            oauth_request_uri: Mutex::new(DEFAULT_OAUTH_REQUEST_URI.to_string()),
            refresh_cancel: Mutex::new(None),
            self_ref: Mutex::new(Weak::new()),
        })
    }

    pub fn initialize(self: &Arc<Self>) -> AuthResult<()> {
        *self.self_ref.lock().unwrap() = Arc::downgrade(self);
        self.restore_from_persistence()?;
        self.install_persistence_subscription()?;
        Ok(())
    }

    pub fn app(&self) -> &FirebaseApp {
        &self.app
    }

    pub fn current_user(&self) -> Option<Arc<User>> {
        self.current_user.lock().unwrap().clone()
    }

    pub fn sign_out(&self) {
        self.clear_local_user_state();
        if let Err(err) = self.set_persisted_state(None) {
            eprintln!("Failed to clear persisted auth state: {err}");
        }
    }

    pub fn email_auth_provider(&self) -> EmailAuthProvider {
        EmailAuthProvider
    }

    pub fn sign_in_with_email_and_password(
        &self,
        email: &str,
        password: &str,
    ) -> AuthResult<UserCredential> {
        let api_key = self.api_key()?;

        let request = SignInWithPasswordRequest {
            email: email.to_owned(),
            password: password.to_owned(),
            return_secure_token: true,
        };

        let response: SignInWithPasswordResponse =
            self.execute_request("accounts:signInWithPassword", &api_key, &request)?;

        let expires_in = self.parse_expires_in(&response.expires_in)?;
        let user = self.build_user_from_response(&response.local_id, &response.email);
        user.update_tokens(
            Some(response.id_token.clone()),
            Some(response.refresh_token.clone()),
            Some(expires_in),
        );
        let user_arc = Arc::new(user);
        *self.current_user.lock().unwrap() = Some(user_arc.clone());
        self.after_token_update(user_arc.clone())?;
        self.listeners.notify(user_arc.clone());

        Ok(UserCredential {
            user: user_arc,
            provider_id: Some(EmailAuthProvider::PROVIDER_ID.to_string()),
            operation_type: Some("signIn".to_string()),
        })
    }

    pub fn create_user_with_email_and_password(
        &self,
        email: &str,
        password: &str,
    ) -> AuthResult<UserCredential> {
        let api_key = self.api_key()?;

        let request = SignUpRequest {
            email: email.to_owned(),
            password: password.to_owned(),
            return_secure_token: true,
        };

        let response: SignUpResponse =
            self.execute_request("accounts:signUp", &api_key, &request)?;

        let user = self.build_user_from_response(&response.local_id, &response.email);
        let expires_in = response
            .expires_in
            .as_ref()
            .map(|expires| self.parse_expires_in(expires))
            .transpose()?;
        user.update_tokens(
            Some(response.id_token.clone()),
            Some(response.refresh_token.clone()),
            expires_in,
        );
        let user_arc = Arc::new(user);
        *self.current_user.lock().unwrap() = Some(user_arc.clone());
        self.after_token_update(user_arc.clone())?;
        self.listeners.notify(user_arc.clone());

        Ok(UserCredential {
            user: user_arc,
            provider_id: Some(EmailAuthProvider::PROVIDER_ID.to_string()),
            operation_type: Some("signUp".to_string()),
        })
    }

    pub fn on_auth_state_changed(
        &self,
        observer: PartialObserver<Arc<User>>,
    ) -> impl FnOnce() + Send + 'static {
        if let Some(user) = self.current_user() {
            if let Some(next) = observer.next.clone() {
                next(&user);
            }
        }

        self.listeners.add_observer(observer);
        || {}
    }

    fn execute_request<TRequest, TResponse>(
        &self,
        path: &str,
        api_key: &str,
        request: &TRequest,
    ) -> AuthResult<TResponse>
    where
        TRequest: Serialize,
        TResponse: serde::de::DeserializeOwned,
    {
        let url = self.endpoint_url(path, api_key)?;
        let response = self
            .rest_client
            .post(url)
            .json(request)
            .send()
            .map_err(|err| AuthError::Network(err.to_string()))?;

        if !response.status().is_success() {
            let message = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AuthError::Network(message));
        }

        response
            .json()
            .map_err(|err| AuthError::Network(err.to_string()))
    }

    fn endpoint_url(&self, path: &str, api_key: &str) -> AuthResult<Url> {
        let base = format!(
            "https://identitytoolkit.googleapis.com/v1/{}?key={}",
            path, api_key
        );
        Url::parse(&base).map_err(|err| AuthError::Network(err.to_string()))
    }

    fn build_user_from_response(&self, local_id: &str, email: &str) -> User {
        let info = UserInfo {
            uid: local_id.to_string(),
            display_name: None,
            email: Some(email.to_string()),
            phone_number: None,
            photo_url: None,
            provider_id: EmailAuthProvider::PROVIDER_ID.to_string(),
        };
        User::new(self.app.clone(), info)
    }

    fn api_key(&self) -> AuthResult<String> {
        self.config
            .api_key
            .clone()
            .ok_or_else(|| AuthError::InvalidCredential("Missing API key".into()))
    }

    fn parse_expires_in(&self, value: &str) -> AuthResult<Duration> {
        let seconds = value.parse::<u64>().map_err(|err| {
            AuthError::InvalidCredential(format!("Invalid expiresIn value: {err}"))
        })?;
        Ok(Duration::from_secs(seconds))
    }

    fn refresh_user_token(&self, user: &Arc<User>) -> AuthResult<String> {
        let refresh_token = user
            .refresh_token()
            .ok_or_else(|| AuthError::InvalidCredential("Missing refresh token".into()))?;
        let api_key = self.api_key()?;
        let response = token::refresh_id_token(&self.rest_client, &api_key, &refresh_token)?;
        let expires_in = self.parse_expires_in(&response.expires_in)?;
        user.update_tokens(
            Some(response.id_token.clone()),
            Some(response.refresh_token.clone()),
            Some(expires_in),
        );
        self.after_token_update(user.clone())?;
        self.listeners.notify(user.clone());
        Ok(response.id_token)
    }

    pub fn get_token(&self, force_refresh: bool) -> AuthResult<Option<String>> {
        let user = match self.current_user() {
            Some(user) => user,
            None => return Ok(None),
        };

        let needs_refresh = force_refresh
            || user
                .token_manager()
                .should_refresh(self.token_refresh_tolerance);

        if needs_refresh {
            self.refresh_user_token(&user).map(Some)
        } else {
            Ok(user.token_manager().access_token())
        }
    }

    pub fn token_provider(self: &Arc<Self>) -> TokenProviderArc {
        crate::auth::token_provider::auth_token_provider_arc(self.clone())
    }

    pub fn set_oauth_request_uri(&self, value: impl Into<String>) {
        *self.oauth_request_uri.lock().unwrap() = value.into();
    }

    pub fn oauth_request_uri(&self) -> String {
        self.oauth_request_uri.lock().unwrap().clone()
    }

    pub fn set_popup_handler(&self, handler: Arc<dyn OAuthPopupHandler>) {
        *self.popup_handler.lock().unwrap() = Some(handler);
    }

    pub fn clear_popup_handler(&self) {
        *self.popup_handler.lock().unwrap() = None;
    }

    pub fn popup_handler(&self) -> Option<Arc<dyn OAuthPopupHandler>> {
        self.popup_handler.lock().unwrap().clone()
    }

    pub fn set_redirect_handler(&self, handler: Arc<dyn OAuthRedirectHandler>) {
        *self.redirect_handler.lock().unwrap() = Some(handler);
    }

    pub fn clear_redirect_handler(&self) {
        *self.redirect_handler.lock().unwrap() = None;
    }

    pub fn redirect_handler(&self) -> Option<Arc<dyn OAuthRedirectHandler>> {
        self.redirect_handler.lock().unwrap().clone()
    }

    pub fn set_redirect_persistence(&self, persistence: Arc<dyn RedirectPersistence>) {
        *self.redirect_persistence.lock().unwrap() = persistence;
    }

    fn redirect_persistence(&self) -> Arc<dyn RedirectPersistence> {
        self.redirect_persistence.lock().unwrap().clone()
    }

    pub fn sign_in_with_oauth_credential(
        &self,
        credential: AuthCredential,
    ) -> AuthResult<UserCredential> {
        self.exchange_oauth_credential(credential, None)
    }

    pub fn send_password_reset_email(&self, email: &str) -> AuthResult<()> {
        let api_key = self.api_key()?;
        send_password_reset_email(&self.rest_client, &api_key, email)
    }

    pub fn confirm_password_reset(&self, oob_code: &str, new_password: &str) -> AuthResult<()> {
        let api_key = self.api_key()?;
        confirm_password_reset(&self.rest_client, &api_key, oob_code, new_password)
    }

    pub fn send_email_verification(&self) -> AuthResult<()> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        let api_key = self.api_key()?;
        send_email_verification(&self.rest_client, &api_key, &id_token)
    }

    pub fn update_profile(
        &self,
        display_name: Option<&str>,
        photo_url: Option<&str>,
    ) -> AuthResult<Arc<User>> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        let mut request = UpdateAccountRequest::new(id_token);
        if let Some(value) = display_name {
            if value.is_empty() {
                request.display_name = Some(UpdateString::Clear);
            } else {
                request.display_name = Some(UpdateString::Set(value.to_string()));
            }
        }
        if let Some(value) = photo_url {
            if value.is_empty() {
                request.photo_url = Some(UpdateString::Clear);
            } else {
                request.photo_url = Some(UpdateString::Set(value.to_string()));
            }
        }

        self.perform_account_update(user, request)
    }

    pub fn update_email(&self, email: &str) -> AuthResult<Arc<User>> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        let mut request = UpdateAccountRequest::new(id_token);
        request.email = Some(email.to_string());
        self.perform_account_update(user, request)
    }

    pub fn update_password(&self, password: &str) -> AuthResult<Arc<User>> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        let mut request = UpdateAccountRequest::new(id_token);
        request.password = Some(password.to_string());
        self.perform_account_update(user, request)
    }

    pub fn delete_user(&self) -> AuthResult<()> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        let api_key = self.api_key()?;
        delete_account(&self.rest_client, &api_key, &id_token)?;
        self.sign_out();
        Ok(())
    }

    pub fn link_with_oauth_credential(
        &self,
        credential: AuthCredential,
    ) -> AuthResult<UserCredential> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        self.exchange_oauth_credential(credential, Some(id_token))
    }

    pub fn reauthenticate_with_password(
        &self,
        email: &str,
        password: &str,
    ) -> AuthResult<Arc<User>> {
        let request = SignInWithPasswordRequest {
            email: email.to_string(),
            password: password.to_string(),
            return_secure_token: true,
        };

        let api_key = self.api_key()?;
        let response = verify_password(&self.rest_client, &api_key, &request)?;
        self.apply_password_reauth(response)
    }

    pub fn reauthenticate_with_oauth_credential(
        &self,
        credential: AuthCredential,
    ) -> AuthResult<Arc<User>> {
        let user = self.require_current_user()?;
        let result = self.exchange_oauth_credential(credential, Some(user.get_id_token(false)?))?;
        Ok(result.user)
    }

    fn exchange_oauth_credential(
        &self,
        credential: AuthCredential,
        id_token: Option<String>,
    ) -> AuthResult<UserCredential> {
        let oauth_credential = OAuthCredential::try_from(credential)?;
        let post_body = oauth_credential.build_post_body()?;
        let request = SignInWithIdpRequest {
            post_body,
            request_uri: self.oauth_request_uri(),
            return_idp_credential: true,
            return_secure_token: true,
            id_token,
        };

        let api_key = self.api_key()?;
        let response = sign_in_with_idp(&self.rest_client, &api_key, &request)?;
        let user_arc = self.upsert_user_from_idp_response(&response, &oauth_credential)?;
        let provider_id = response
            .provider_id
            .clone()
            .or_else(|| Some(oauth_credential.provider_id().to_string()))
            .unwrap_or_else(|| EmailAuthProvider::PROVIDER_ID.to_string());

        self.listeners.notify(user_arc.clone());

        Ok(UserCredential {
            user: user_arc,
            provider_id: Some(provider_id),
            operation_type: Some(if response.is_new_user.unwrap_or(false) {
                "signUp".to_string()
            } else {
                "signIn".to_string()
            }),
        })
    }

    fn perform_account_update(
        &self,
        current_user: Arc<User>,
        request: UpdateAccountRequest,
    ) -> AuthResult<Arc<User>> {
        let api_key = self.api_key()?;
        let response = update_account(&self.rest_client, &api_key, &request)?;
        let updated_user = self.apply_account_update(&current_user, &response)?;
        self.listeners.notify(updated_user.clone());
        Ok(updated_user)
    }

    fn require_current_user(&self) -> AuthResult<Arc<User>> {
        self.current_user()
            .ok_or_else(|| AuthError::InvalidCredential("No user signed in".into()))
    }

    pub(crate) fn set_pending_redirect_event(
        &self,
        provider_id: &str,
        operation: RedirectOperation,
    ) -> AuthResult<()> {
        let event = PendingRedirectEvent {
            provider_id: provider_id.to_string(),
            operation,
        };
        self.redirect_persistence().set(Some(event))
    }

    pub(crate) fn clear_pending_redirect_event(&self) -> AuthResult<()> {
        self.redirect_persistence().set(None)
    }

    pub(crate) fn take_pending_redirect_event(&self) -> AuthResult<Option<PendingRedirectEvent>> {
        let event = self.redirect_persistence().get()?;
        if event.is_some() {
            self.redirect_persistence().set(None)?;
        }
        Ok(event)
    }

    fn apply_password_reauth(&self, response: SignInWithPasswordResponse) -> AuthResult<Arc<User>> {
        let user = self.build_user_from_response(&response.local_id, &response.email);
        let expires_in = self.parse_expires_in(&response.expires_in)?;
        user.update_tokens(
            Some(response.id_token.clone()),
            Some(response.refresh_token.clone()),
            Some(expires_in),
        );

        let user_arc = Arc::new(user);
        *self.current_user.lock().unwrap() = Some(user_arc.clone());
        self.after_token_update(user_arc.clone())?;
        Ok(user_arc)
    }

    fn restore_from_persistence(&self) -> AuthResult<()> {
        let state = self.persistence.get()?;
        let notify = state.is_some();
        self.sync_from_persistence(state, notify)
    }

    fn after_token_update(&self, user: Arc<User>) -> AuthResult<()> {
        self.save_persisted_state(&user)?;
        self.schedule_refresh_for_user(user);
        Ok(())
    }

    fn save_persisted_state(&self, user: &Arc<User>) -> AuthResult<()> {
        let refresh_token = match user.refresh_token() {
            Some(token) if !token.is_empty() => Some(token),
            _ => {
                self.set_persisted_state(None)?;
                return Ok(());
            }
        };

        let expires_at = user
            .token_manager()
            .expiration_time()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64);

        let state = PersistedAuthState {
            user_id: user.uid().to_string(),
            email: user.info().email.clone(),
            refresh_token,
            access_token: user.token_manager().access_token(),
            expires_at,
        };
        self.set_persisted_state(Some(state))
    }

    fn set_persisted_state(&self, state: Option<PersistedAuthState>) -> AuthResult<()> {
        {
            let cache = self.persisted_state_cache.lock().unwrap();
            if *cache == state {
                return Ok(());
            }
        }

        let previous = self.update_cached_state(state.clone());
        if let Err(err) = self.persistence.set(state) {
            self.update_cached_state(previous);
            return Err(err);
        }
        Ok(())
    }

    fn update_cached_state(&self, state: Option<PersistedAuthState>) -> Option<PersistedAuthState> {
        let mut guard = self.persisted_state_cache.lock().unwrap();
        std::mem::replace(&mut *guard, state)
    }

    fn install_persistence_subscription(self: &Arc<Self>) -> AuthResult<()> {
        let weak = Arc::downgrade(self);
        let listener: PersistenceListener = Arc::new(move |state: Option<PersistedAuthState>| {
            if let Some(auth) = weak.upgrade() {
                if let Err(err) = auth.sync_from_persistence(state, true) {
                    eprintln!("Failed to sync persisted auth state: {err}");
                }
            }
        });

        let subscription = self.persistence.subscribe(listener)?;
        *self.persistence_subscription.lock().unwrap() = Some(subscription);
        Ok(())
    }

    fn sync_from_persistence(
        &self,
        state: Option<PersistedAuthState>,
        notify_listeners: bool,
    ) -> AuthResult<()> {
        {
            let cache = self.persisted_state_cache.lock().unwrap();
            if *cache == state {
                return Ok(());
            }
        }

        match state.clone() {
            Some(ref persisted) if Self::has_refresh_token(persisted) => {
                let user_arc = self.build_user_from_persisted_state(persisted);
                *self.current_user.lock().unwrap() = Some(user_arc.clone());
                self.schedule_refresh_for_user(user_arc.clone());
                if notify_listeners {
                    self.listeners.notify(user_arc);
                }
            }
            _ => {
                self.clear_local_user_state();
            }
        }

        self.update_cached_state(state);
        Ok(())
    }

    fn build_user_from_persisted_state(&self, state: &PersistedAuthState) -> Arc<User> {
        let info = UserInfo {
            uid: state.user_id.clone(),
            display_name: None,
            email: state.email.clone(),
            phone_number: None,
            photo_url: None,
            provider_id: EmailAuthProvider::PROVIDER_ID.to_string(),
        };

        let user = User::new(self.app.clone(), info);
        let expiration_time = state.expires_at.and_then(|seconds| {
            if seconds <= 0 {
                None
            } else {
                UNIX_EPOCH.checked_add(Duration::from_secs(seconds as u64))
            }
        });

        user.token_manager().initialize(
            state.access_token.clone(),
            state.refresh_token.clone(),
            expiration_time,
        );

        Arc::new(user)
    }

    fn clear_local_user_state(&self) {
        self.cancel_scheduled_refresh();
        let mut guard = self.current_user.lock().unwrap();
        if let Some(user) = guard.as_ref() {
            user.token_manager().clear();
        }
        *guard = None;
    }

    fn has_refresh_token(state: &PersistedAuthState) -> bool {
        state
            .refresh_token
            .as_ref()
            .map(|token| !token.is_empty())
            .unwrap_or(false)
    }

    fn upsert_user_from_idp_response(
        &self,
        response: &SignInWithIdpResponse,
        oauth_credential: &OAuthCredential,
    ) -> AuthResult<Arc<User>> {
        let id_token = response.id_token.clone().ok_or_else(|| {
            AuthError::InvalidCredential("signInWithIdp response missing idToken".into())
        })?;
        let refresh_token = response.refresh_token.clone().ok_or_else(|| {
            AuthError::InvalidCredential("signInWithIdp response missing refreshToken".into())
        })?;
        let local_id = response.local_id.clone().ok_or_else(|| {
            AuthError::InvalidCredential("signInWithIdp response missing localId".into())
        })?;

        let provider_id = response
            .provider_id
            .clone()
            .unwrap_or_else(|| oauth_credential.provider_id().to_string());

        let display_name = oauth_credential
            .token_response()
            .get("displayName")
            .and_then(Value::as_str)
            .map(|value| value.to_string());
        let photo_url = oauth_credential
            .token_response()
            .get("photoUrl")
            .and_then(Value::as_str)
            .map(|value| value.to_string());

        let info = UserInfo {
            uid: local_id,
            display_name,
            email: response.email.clone(),
            phone_number: None,
            photo_url,
            provider_id,
        };

        let user = User::new(self.app.clone(), info);
        let expires_in = response
            .expires_in
            .as_deref()
            .map(|value| self.parse_expires_in(value))
            .transpose()?;
        user.update_tokens(Some(id_token), Some(refresh_token), expires_in);

        let user_arc = Arc::new(user);
        *self.current_user.lock().unwrap() = Some(user_arc.clone());
        self.after_token_update(user_arc.clone())?;
        Ok(user_arc)
    }

    fn apply_account_update(
        &self,
        current_user: &Arc<User>,
        response: &UpdateAccountResponse,
    ) -> AuthResult<Arc<User>> {
        let id_token = response.id_token.clone().ok_or_else(|| {
            AuthError::InvalidCredential("accounts:update response missing idToken".into())
        })?;
        let refresh_token = response.refresh_token.clone().ok_or_else(|| {
            AuthError::InvalidCredential("accounts:update response missing refreshToken".into())
        })?;

        let expires_in = response
            .expires_in
            .as_deref()
            .map(|value| self.parse_expires_in(value))
            .transpose()?;

        let uid = response
            .local_id
            .clone()
            .unwrap_or_else(|| current_user.uid().to_string());

        let email = response
            .email
            .clone()
            .or_else(|| current_user.info().email.clone());

        let display_name = response
            .display_name
            .as_deref()
            .map(|value| {
                if value.is_empty() {
                    None
                } else {
                    Some(value.to_string())
                }
            })
            .flatten()
            .or_else(|| current_user.info().display_name.clone());

        let photo_url = response
            .photo_url
            .as_deref()
            .map(|value| {
                if value.is_empty() {
                    None
                } else {
                    Some(value.to_string())
                }
            })
            .flatten()
            .or_else(|| current_user.info().photo_url.clone());

        let provider_id = response
            .provider_user_info
            .as_ref()
            .and_then(|infos| infos.first())
            .and_then(|info| info.provider_id.clone())
            .unwrap_or_else(|| current_user.info().provider_id.clone());

        let info = UserInfo {
            uid,
            display_name,
            email,
            phone_number: current_user.info().phone_number.clone(),
            photo_url,
            provider_id,
        };

        let user = User::new(self.app.clone(), info);
        user.update_tokens(Some(id_token), Some(refresh_token), expires_in);

        let user_arc = Arc::new(user);
        *self.current_user.lock().unwrap() = Some(user_arc.clone());
        self.after_token_update(user_arc.clone())?;
        Ok(user_arc)
    }

    fn schedule_refresh_for_user(&self, user: Arc<User>) {
        if user.refresh_token().is_none() {
            self.cancel_scheduled_refresh();
            return;
        }

        let Some(expiration_time) = user.token_manager().expiration_time() else {
            self.cancel_scheduled_refresh();
            return;
        };

        let now = SystemTime::now();
        let expires_in = match expiration_time.duration_since(now) {
            Ok(duration) => duration,
            Err(_) => Duration::from_secs(0),
        };

        let delay = if expires_in > self.token_refresh_tolerance {
            expires_in - self.token_refresh_tolerance
        } else {
            Duration::from_secs(0)
        };

        let Some(self_arc) = self.self_arc() else {
            return;
        };

        let cancel_flag = Arc::new(AtomicBool::new(false));
        {
            let mut guard = self.refresh_cancel.lock().unwrap();
            if let Some(flag) = guard.take() {
                flag.store(true, Ordering::SeqCst);
            }
            *guard = Some(cancel_flag.clone());
        }

        let user_arc = user.clone();
        thread::spawn(move || {
            if !sleep_with_cancel(delay, &cancel_flag) {
                return;
            }

            let mut attempts = 0u32;
            loop {
                if cancel_flag.load(Ordering::SeqCst) {
                    return;
                }

                match self_arc.refresh_user_token(&user_arc) {
                    Ok(_) => return,
                    Err(err) => {
                        attempts = attempts.saturating_add(1);
                        let wait = backoff::calculate_backoff_millis(attempts);
                        eprintln!("Auth token refresh failed (attempt {attempts}): {err}");
                        if !sleep_with_cancel(Duration::from_millis(wait), &cancel_flag) {
                            return;
                        }
                    }
                }
            }
        });
    }

    fn cancel_scheduled_refresh(&self) {
        if let Some(flag) = self.refresh_cancel.lock().unwrap().take() {
            flag.store(true, Ordering::SeqCst);
        }
    }

    fn self_arc(&self) -> Option<Arc<Auth>> {
        self.self_ref.lock().unwrap().upgrade()
    }
}

pub struct AuthBuilder {
    app: FirebaseApp,
    persistence: Option<Arc<dyn AuthPersistence + Send + Sync>>,
    auto_initialize: bool,
    popup_handler: Option<Arc<dyn OAuthPopupHandler>>,
    redirect_handler: Option<Arc<dyn OAuthRedirectHandler>>,
    oauth_request_uri: Option<String>,
    redirect_persistence: Option<Arc<dyn RedirectPersistence>>,
}

impl AuthBuilder {
    fn new(app: FirebaseApp) -> Self {
        Self {
            app,
            persistence: None,
            auto_initialize: true,
            popup_handler: None,
            redirect_handler: None,
            oauth_request_uri: None,
            redirect_persistence: None,
        }
    }

    pub fn with_persistence(mut self, persistence: Arc<dyn AuthPersistence + Send + Sync>) -> Self {
        self.persistence = Some(persistence);
        self
    }

    pub fn with_popup_handler(mut self, handler: Arc<dyn OAuthPopupHandler>) -> Self {
        self.popup_handler = Some(handler);
        self
    }

    pub fn with_redirect_handler(mut self, handler: Arc<dyn OAuthRedirectHandler>) -> Self {
        self.redirect_handler = Some(handler);
        self
    }

    pub fn with_oauth_request_uri(mut self, request_uri: impl Into<String>) -> Self {
        self.oauth_request_uri = Some(request_uri.into());
        self
    }

    pub fn with_redirect_persistence(mut self, persistence: Arc<dyn RedirectPersistence>) -> Self {
        self.redirect_persistence = Some(persistence);
        self
    }

    pub fn defer_initialization(mut self) -> Self {
        self.auto_initialize = false;
        self
    }

    pub fn build(self) -> AuthResult<Arc<Auth>> {
        let persistence = self
            .persistence
            .unwrap_or_else(|| Arc::new(InMemoryPersistence::default()));
        let auth = Arc::new(Auth::new_with_persistence(self.app, persistence)?);
        if let Some(handler) = self.popup_handler {
            auth.set_popup_handler(handler);
        }
        if let Some(handler) = self.redirect_handler {
            auth.set_redirect_handler(handler);
        }
        if let Some(request_uri) = self.oauth_request_uri {
            auth.set_oauth_request_uri(request_uri);
        }
        if let Some(persistence) = self.redirect_persistence {
            auth.set_redirect_persistence(persistence);
        }
        if self.auto_initialize {
            auth.initialize()?;
        }
        Ok(auth)
    }
}

pub fn register_auth_component() {
    use std::sync::LazyLock;
    static REGISTERED: LazyLock<()> = LazyLock::new(|| {
        let component = Component::new("auth", Arc::new(auth_factory), ComponentType::Public)
            .with_instantiation_mode(InstantiationMode::Lazy);
        let _ = crate::component::register_component(component);
    });
    LazyLock::force(&REGISTERED);
}

fn auth_factory(
    container: &ComponentContainer,
    _options: InstanceFactoryOptions,
) -> Result<DynService, ComponentError> {
    let app = container.root_service::<FirebaseApp>().ok_or_else(|| {
        ComponentError::InitializationFailed {
            name: "auth".to_string(),
            reason: "Firebase app not attached to component container".to_string(),
        }
    })?;
    let auth = Auth::new((*app).clone()).map_err(|err| ComponentError::InitializationFailed {
        name: "auth".to_string(),
        reason: err.to_string(),
    })?;
    let auth = Arc::new(auth);
    auth.initialize()
        .map_err(|err| ComponentError::InitializationFailed {
            name: "auth".to_string(),
            reason: err.to_string(),
        })?;
    Ok(auth as DynService)
}

pub fn auth_for_app(app: FirebaseApp) -> AuthResult<Arc<Auth>> {
    let provider = app.container().get_provider("auth");
    provider.get_immediate::<Auth>().ok_or_else(|| {
        AuthError::App(AppError::ComponentFailure {
            component: "auth".to_string(),
            message: "Auth service not initialized".to_string(),
        })
    })
}

fn sleep_with_cancel(mut duration: Duration, cancel_flag: &AtomicBool) -> bool {
    while duration > Duration::ZERO {
        if cancel_flag.load(Ordering::SeqCst) {
            return false;
        }
        let step = if duration > Duration::from_millis(250) {
            Duration::from_millis(250)
        } else {
            duration
        };
        thread::sleep(step);
        duration = duration.checked_sub(step).unwrap_or(Duration::ZERO);
    }

    !cancel_flag.load(Ordering::SeqCst)
}
