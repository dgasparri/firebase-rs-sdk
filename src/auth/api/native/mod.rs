use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, UNIX_EPOCH};

use async_trait::async_trait;
use reqwest::Client;
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
    AuthConfig, AuthCredential, AuthStateListeners, EmailAuthProvider, GetAccountInfoResponse,
    SignInWithPasswordRequest, SignInWithPasswordResponse, SignUpRequest, SignUpResponse, User,
    UserCredential, UserInfo,
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
#[cfg(feature = "firestore")]
use crate::firestore::remote::datastore::TokenProviderArc;
use crate::platform::token::{AsyncTokenProvider, TokenError};
use crate::util::PartialObserver;
use account::{
    confirm_password_reset, delete_account, get_account_info, send_email_verification,
    send_password_reset_email, update_account, verify_password, UpdateAccountRequest,
    UpdateAccountResponse, UpdateString,
};
use idp::{sign_in_with_idp, SignInWithIdpRequest, SignInWithIdpResponse};

const DEFAULT_OAUTH_REQUEST_URI: &str = "http://localhost";
const DEFAULT_IDENTITY_TOOLKIT_ENDPOINT: &str = "https://identitytoolkit.googleapis.com/v1";

pub struct Auth {
    app: FirebaseApp,
    config: Mutex<AuthConfig>,
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
    identity_toolkit_endpoint: Mutex<String>,
    secure_token_endpoint: Mutex<String>,
    refresh_cancel: Mutex<Option<Arc<AtomicBool>>>,
    self_ref: Mutex<Weak<Auth>>,
}

#[async_trait]
impl AsyncTokenProvider for Arc<Auth> {
    async fn get_token(&self, force_refresh: bool) -> Result<Option<String>, TokenError> {
        self.get_token(force_refresh)
            .await
            .map_err(TokenError::from_error)
    }
}

impl Auth {
    /// Creates a builder for configuring an `Auth` instance before construction.
    pub fn builder(app: FirebaseApp) -> AuthBuilder {
        AuthBuilder::new(app)
    }

    /// Constructs an `Auth` instance using in-memory persistence.
    pub fn new(app: FirebaseApp) -> AuthResult<Self> {
        #[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
        let persistence: Arc<dyn AuthPersistence + Send + Sync> =
            Arc::new(crate::auth::persistence::IndexedDbPersistence::new());

        #[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
        let persistence: Arc<dyn AuthPersistence + Send + Sync> =
            Arc::new(InMemoryPersistence::default());
        Self::new_with_persistence(app, persistence)
    }

    /// Constructs an `Auth` instance with a caller-provided persistence backend.
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
            identity_toolkit_endpoint: Some(DEFAULT_IDENTITY_TOOLKIT_ENDPOINT.to_string()),
            secure_token_endpoint: Some(token::DEFAULT_SECURE_TOKEN_ENDPOINT.to_string()),
        };

        Ok(Self {
            app,
            config: Mutex::new(config),
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
            identity_toolkit_endpoint: Mutex::new(DEFAULT_IDENTITY_TOOLKIT_ENDPOINT.to_string()),
            secure_token_endpoint: Mutex::new(token::DEFAULT_SECURE_TOKEN_ENDPOINT.to_string()),
            refresh_cancel: Mutex::new(None),
            self_ref: Mutex::new(Weak::new()),
        })
    }

    /// Finishes initialization by restoring persisted state and wiring listeners.
    pub fn initialize(self: &Arc<Self>) -> AuthResult<()> {
        *self.self_ref.lock().unwrap() = Arc::downgrade(self);
        self.restore_from_persistence()?;
        self.install_persistence_subscription()?;
        Ok(())
    }

    /// Returns the `FirebaseApp` associated with this Auth instance.
    pub fn app(&self) -> &FirebaseApp {
        &self.app
    }

    /// Returns the currently signed-in user, if any.
    pub fn current_user(&self) -> Option<Arc<User>> {
        self.current_user.lock().unwrap().clone()
    }

    /// Signs out the current user and clears persisted credentials.
    pub fn sign_out(&self) {
        self.clear_local_user_state();
        if let Err(err) = self.set_persisted_state(None) {
            eprintln!("Failed to clear persisted auth state: {err}");
        }
    }

    /// Returns the email/password auth provider helper.
    pub fn email_auth_provider(&self) -> EmailAuthProvider {
        EmailAuthProvider
    }

    /// Signs a user in using the email/password REST endpoint.
    pub async fn sign_in_with_email_and_password(
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

        let response: SignInWithPasswordResponse = self
            .execute_request("accounts:signInWithPassword", &api_key, &request)
            .await?;

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

    /// Creates a new user using email/password credentials.
    pub async fn create_user_with_email_and_password(
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

        let response: SignUpResponse = self
            .execute_request("accounts:signUp", &api_key, &request)
            .await?;

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

    /// Registers an observer that is invoked whenever auth state changes.
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

    async fn execute_request<TRequest, TResponse>(
        &self,
        path: &str,
        api_key: &str,
        request: &TRequest,
    ) -> AuthResult<TResponse>
    where
        TRequest: Serialize,
        TResponse: serde::de::DeserializeOwned + 'static,
    {
        let client = self.rest_client.clone();
        let url = self.endpoint_url(path, api_key)?;
        let body =
            serde_json::to_vec(request).map_err(|err| AuthError::Network(err.to_string()))?;

        let response = client
            .post(url)
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(|err| AuthError::Network(err.to_string()))?;

        if !response.status().is_success() {
            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AuthError::Network(message));
        }

        response
            .json()
            .await
            .map_err(|err| AuthError::Network(err.to_string()))
    }

    fn endpoint_url(&self, path: &str, api_key: &str) -> AuthResult<Url> {
        let base = self.identity_toolkit_endpoint();
        let endpoint = format!("{}/{}?key={}", base.trim_end_matches('/'), path, api_key);
        Url::parse(&endpoint).map_err(|err| AuthError::Network(err.to_string()))
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
            .lock()
            .unwrap()
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

    async fn refresh_user_token(&self, user: &Arc<User>) -> AuthResult<String> {
        let refresh_token = user
            .refresh_token()
            .ok_or_else(|| AuthError::InvalidCredential("Missing refresh token".into()))?;
        let api_key = self.api_key()?;
        let secure_endpoint = self.secure_token_endpoint();
        let response = token::refresh_id_token_with_endpoint(
            &self.rest_client,
            &secure_endpoint,
            &api_key,
            &refresh_token,
        )
        .await?;
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

    /// Returns the current user's ID token, refreshing when requested.
    pub async fn get_token(&self, force_refresh: bool) -> AuthResult<Option<String>> {
        let user = match self.current_user() {
            Some(user) => user,
            None => return Ok(None),
        };

        let needs_refresh = force_refresh
            || user
                .token_manager()
                .should_refresh(self.token_refresh_tolerance);

        if needs_refresh {
            let token = self.refresh_user_token(&user).await?;
            Ok(Some(token))
        } else {
            Ok(user.token_manager().access_token())
        }
    }

    pub async fn get_token_async(&self, force_refresh: bool) -> AuthResult<Option<String>> {
        self.get_token(force_refresh).await
    }

    /// Exposes this auth instance as a Firestore token provider.
    #[cfg(feature = "firestore")]
    pub fn token_provider(self: &Arc<Self>) -> TokenProviderArc {
        crate::auth::token_provider::auth_token_provider_arc(self.clone())
    }

    /// Overrides the default OAuth request URI used during flows.
    pub fn set_oauth_request_uri(&self, value: impl Into<String>) {
        *self.oauth_request_uri.lock().unwrap() = value.into();
    }

    /// Returns the OAuth request URI for popup/redirect flows.
    pub fn oauth_request_uri(&self) -> String {
        self.oauth_request_uri.lock().unwrap().clone()
    }

    /// Updates the Identity Toolkit REST endpoint.
    pub fn set_identity_toolkit_endpoint(&self, endpoint: impl Into<String>) {
        let value = endpoint.into();
        *self.identity_toolkit_endpoint.lock().unwrap() = value.clone();
        self.config.lock().unwrap().identity_toolkit_endpoint = Some(value);
    }

    /// Returns the Identity Toolkit REST endpoint in use.
    pub fn identity_toolkit_endpoint(&self) -> String {
        self.identity_toolkit_endpoint.lock().unwrap().clone()
    }

    /// Sets the Secure Token endpoint used for refresh operations.
    pub fn set_secure_token_endpoint(&self, endpoint: impl Into<String>) {
        let value = endpoint.into();
        *self.secure_token_endpoint.lock().unwrap() = value.clone();
        self.config.lock().unwrap().secure_token_endpoint = Some(value);
    }

    fn secure_token_endpoint(&self) -> String {
        self.secure_token_endpoint.lock().unwrap().clone()
    }

    /// Installs an OAuth popup handler implementation.
    pub fn set_popup_handler(&self, handler: Arc<dyn OAuthPopupHandler>) {
        *self.popup_handler.lock().unwrap() = Some(handler);
    }

    /// Clears any installed popup handler.
    pub fn clear_popup_handler(&self) {
        *self.popup_handler.lock().unwrap() = None;
    }

    /// Retrieves the currently configured popup handler.
    pub fn popup_handler(&self) -> Option<Arc<dyn OAuthPopupHandler>> {
        self.popup_handler.lock().unwrap().clone()
    }

    /// Installs an OAuth redirect handler implementation.
    pub fn set_redirect_handler(&self, handler: Arc<dyn OAuthRedirectHandler>) {
        *self.redirect_handler.lock().unwrap() = Some(handler);
    }

    /// Clears any installed redirect handler.
    pub fn clear_redirect_handler(&self) {
        *self.redirect_handler.lock().unwrap() = None;
    }

    /// Retrieves the currently configured redirect handler.
    pub fn redirect_handler(&self) -> Option<Arc<dyn OAuthRedirectHandler>> {
        self.redirect_handler.lock().unwrap().clone()
    }

    /// Replaces the persistence mechanism used for redirect state.
    pub fn set_redirect_persistence(&self, persistence: Arc<dyn RedirectPersistence>) {
        *self.redirect_persistence.lock().unwrap() = persistence;
    }

    fn redirect_persistence(&self) -> Arc<dyn RedirectPersistence> {
        self.redirect_persistence.lock().unwrap().clone()
    }

    /// Signs in using an OAuth credential produced by popup/redirect flows.
    pub fn sign_in_with_oauth_credential(
        &self,
        credential: AuthCredential,
    ) -> AuthResult<UserCredential> {
        self.exchange_oauth_credential(credential, None)
    }

    /// Sends a password reset email to the specified address.
    pub fn send_password_reset_email(&self, email: &str) -> AuthResult<()> {
        let api_key = self.api_key()?;
        let endpoint = self.identity_toolkit_endpoint();
        send_password_reset_email(&self.rest_client, &endpoint, &api_key, email)
    }

    /// Confirms a password reset OOB code and applies the new password.
    pub fn confirm_password_reset(&self, oob_code: &str, new_password: &str) -> AuthResult<()> {
        let api_key = self.api_key()?;
        let endpoint = self.identity_toolkit_endpoint();
        confirm_password_reset(
            &self.rest_client,
            &endpoint,
            &api_key,
            oob_code,
            new_password,
        )
    }

    /// Sends an email verification message to the currently signed-in user.
    pub fn send_email_verification(&self) -> AuthResult<()> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        let api_key = self.api_key()?;
        let endpoint = self.identity_toolkit_endpoint();
        send_email_verification(&self.rest_client, &endpoint, &api_key, &id_token)
    }

    /// Updates the current user's display name and photo URL.
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

    /// Updates the current user's email address.
    pub fn update_email(&self, email: &str) -> AuthResult<Arc<User>> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        let mut request = UpdateAccountRequest::new(id_token);
        request.email = Some(email.to_string());
        self.perform_account_update(user, request)
    }

    /// Updates the current user's password.
    pub fn update_password(&self, password: &str) -> AuthResult<Arc<User>> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        let mut request = UpdateAccountRequest::new(id_token);
        request.password = Some(password.to_string());
        self.perform_account_update(user, request)
    }

    /// Deletes the current user from Firebase Auth.
    pub fn delete_user(&self) -> AuthResult<()> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        let api_key = self.api_key()?;
        let endpoint = self.identity_toolkit_endpoint();
        delete_account(&self.rest_client, &endpoint, &api_key, &id_token)?;
        self.sign_out();
        Ok(())
    }

    /// Unlinks the specified providers from the current user.
    pub fn unlink_providers(&self, provider_ids: &[&str]) -> AuthResult<Arc<User>> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        let mut request = UpdateAccountRequest::new(id_token);
        request.delete_providers = provider_ids.iter().map(|id| id.to_string()).collect();
        self.perform_account_update(user, request)
    }

    /// Fetches the latest account info for the current user.
    pub fn get_account_info(&self) -> AuthResult<GetAccountInfoResponse> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        let api_key = self.api_key()?;
        let endpoint = self.identity_toolkit_endpoint();
        get_account_info(&self.rest_client, &endpoint, &api_key, &id_token)
    }

    /// Links an OAuth credential with the currently signed-in user.
    pub fn link_with_oauth_credential(
        &self,
        credential: AuthCredential,
    ) -> AuthResult<UserCredential> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        self.exchange_oauth_credential(credential, Some(id_token))
    }

    /// Reauthenticates the current user with email and password.
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
        let endpoint = self.identity_toolkit_endpoint();
        let response = verify_password(&self.rest_client, &endpoint, &api_key, &request)?;
        self.apply_password_reauth(response)
    }

    /// Reauthenticates the current user with an OAuth credential.
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
        let endpoint = self.identity_toolkit_endpoint();
        let response = update_account(&self.rest_client, &endpoint, &api_key, &request)?;
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

        let display_name = match response.display_name.as_deref() {
            Some(value) if value.is_empty() => None,
            Some(value) => Some(value.to_string()),
            None => current_user.info().display_name.clone(),
        };

        let photo_url = match response.photo_url.as_deref() {
            Some(value) if value.is_empty() => None,
            Some(value) => Some(value.to_string()),
            None => current_user.info().photo_url.clone(),
        };

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

    fn schedule_refresh_for_user(&self, _user: Arc<User>) {
        // TODO(async-wasm): Reintroduce async timer-based refresh once the shared runtime utilities land.
        self.cancel_scheduled_refresh();
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
    identity_toolkit_endpoint: Option<String>,
    secure_token_endpoint: Option<String>,
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
            identity_toolkit_endpoint: None,
            secure_token_endpoint: None,
        }
    }

    /// Overrides the persistence backend used by the Auth instance.
    pub fn with_persistence(mut self, persistence: Arc<dyn AuthPersistence + Send + Sync>) -> Self {
        self.persistence = Some(persistence);
        self
    }

    /// Installs a popup handler prior to building the Auth instance.
    pub fn with_popup_handler(mut self, handler: Arc<dyn OAuthPopupHandler>) -> Self {
        self.popup_handler = Some(handler);
        self
    }

    /// Installs a redirect handler prior to building the Auth instance.
    pub fn with_redirect_handler(mut self, handler: Arc<dyn OAuthRedirectHandler>) -> Self {
        self.redirect_handler = Some(handler);
        self
    }

    /// Overrides the default OAuth request URI before building.
    pub fn with_oauth_request_uri(mut self, request_uri: impl Into<String>) -> Self {
        self.oauth_request_uri = Some(request_uri.into());
        self
    }

    /// Configures the redirect persistence implementation used post-build.
    pub fn with_redirect_persistence(mut self, persistence: Arc<dyn RedirectPersistence>) -> Self {
        self.redirect_persistence = Some(persistence);
        self
    }

    /// Overrides the Identity Toolkit endpoint used by the Auth instance.
    pub fn with_identity_toolkit_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.identity_toolkit_endpoint = Some(endpoint.into());
        self
    }

    /// Overrides the Secure Token endpoint used for refresh operations.
    pub fn with_secure_token_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.secure_token_endpoint = Some(endpoint.into());
        self
    }

    /// Prevents `build` from automatically calling `initialize`.
    pub fn defer_initialization(mut self) -> Self {
        self.auto_initialize = false;
        self
    }

    /// Builds the Auth instance, applying all configured overrides.
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
        if let Some(endpoint) = self.identity_toolkit_endpoint {
            auth.set_identity_toolkit_endpoint(endpoint);
        }
        if let Some(endpoint) = self.secure_token_endpoint {
            auth.set_secure_token_endpoint(endpoint);
        }
        if self.auto_initialize {
            auth.initialize()?;
        }
        Ok(auth)
    }
}

/// Registers the Auth component so apps can resolve `Auth` instances.
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

/// Retrieves the `Auth` service for the provided app, initializing if needed.
pub fn auth_for_app(app: FirebaseApp) -> AuthResult<Arc<Auth>> {
    let provider = app.container().get_provider("auth");
    provider.get_immediate::<Auth>().ok_or_else(|| {
        AuthError::App(AppError::ComponentFailure {
            component: "auth".to_string(),
            message: "Auth service not initialized".to_string(),
        })
    })
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::test_support::{start_mock_server, test_firebase_app_with_api_key};
    use futures::executor::block_on;
    use httpmock::prelude::*;
    use serde_json::json;

    const TEST_API_KEY: &str = "test-api-key";
    const TEST_EMAIL: &str = "user@example.com";
    const TEST_PASSWORD: &str = "secret";
    const TEST_UID: &str = "uid-123";
    const TEST_ID_TOKEN: &str = "id-token";
    const TEST_REFRESH_TOKEN: &str = "refresh-token";
    const REAUTH_EMAIL: &str = "reauth@example.com";
    const REAUTH_ID_TOKEN: &str = "reauth-id-token";
    const REAUTH_REFRESH_TOKEN: &str = "reauth-refresh-token";
    const REAUTH_UID: &str = "reauth-uid";
    const GOOGLE_PROVIDER_ID: &str = "google.com";
    const UPDATED_ID_TOKEN: &str = "updated-id-token";
    const UPDATED_REFRESH_TOKEN: &str = "updated-refresh-token";

    fn build_auth(server: &MockServer) -> Arc<Auth> {
        Auth::builder(test_firebase_app_with_api_key(TEST_API_KEY))
            .with_identity_toolkit_endpoint(server.url("/v1"))
            .with_secure_token_endpoint(server.url("/token"))
            .defer_initialization()
            .build()
            .expect("failed to build auth")
    }

    fn sign_in_user(auth: &Arc<Auth>, server: &MockServer) {
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:signInWithPassword")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "email": TEST_EMAIL,
                    "password": TEST_PASSWORD,
                    "returnSecureToken": true
                }));
            then.status(200).json_body(json!({
                "localId": TEST_UID,
                "email": TEST_EMAIL,
                "idToken": TEST_ID_TOKEN,
                "refreshToken": TEST_REFRESH_TOKEN,
                "expiresIn": "3600"
            }));
        });

        block_on(auth.sign_in_with_email_and_password(TEST_EMAIL, TEST_PASSWORD))
            .expect("sign-in should succeed");
        mock.assert();
    }

    #[test]
    fn sign_in_with_email_and_password_success() {
        let server = start_mock_server();
        let auth = build_auth(&server);

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:signInWithPassword")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "email": "user@example.com",
                    "password": "secret",
                    "returnSecureToken": true
                }));
            then.status(200).json_body(json!({
                "localId": "uid-123",
                "email": "user@example.com",
                "idToken": "id-token",
                "refreshToken": "refresh-token",
                "expiresIn": "3600"
            }));
        });

        let credential =
            block_on(auth.sign_in_with_email_and_password("user@example.com", "secret"))
                .expect("sign-in should succeed");

        mock.assert();
        assert_eq!(
            credential.provider_id.as_deref(),
            Some(EmailAuthProvider::PROVIDER_ID)
        );
        assert_eq!(credential.operation_type.as_deref(), Some("signIn"));
        assert_eq!(credential.user.uid(), "uid-123");
        assert_eq!(
            credential.user.token_manager().access_token(),
            Some("id-token".to_string())
        );
        assert_eq!(
            credential.user.refresh_token(),
            Some("refresh-token".to_string())
        );
    }

    #[test]
    fn create_user_with_email_and_password_success() {
        let server = start_mock_server();
        let auth = build_auth(&server);

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:signUp")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "email": "user@example.com",
                    "password": "secret",
                    "returnSecureToken": true
                }));
            then.status(200).json_body(json!({
                "localId": "uid-456",
                "email": "user@example.com",
                "idToken": "new-id-token",
                "refreshToken": "new-refresh-token",
                "expiresIn": "7200"
            }));
        });

        let credential =
            block_on(auth.create_user_with_email_and_password("user@example.com", "secret"))
                .expect("sign-up should succeed");

        mock.assert();
        assert_eq!(
            credential.provider_id.as_deref(),
            Some(EmailAuthProvider::PROVIDER_ID)
        );
        assert_eq!(credential.operation_type.as_deref(), Some("signUp"));
        assert_eq!(credential.user.uid(), "uid-456");
        assert_eq!(
            credential.user.token_manager().access_token(),
            Some("new-id-token".to_string())
        );
        assert_eq!(
            credential.user.refresh_token(),
            Some("new-refresh-token".to_string())
        );
    }

    #[test]
    fn sign_in_with_invalid_expires_in_returns_error() {
        let server = start_mock_server();
        let auth = build_auth(&server);

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:signInWithPassword")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "email": "user@example.com",
                    "password": "secret",
                    "returnSecureToken": true
                }));
            then.status(200).json_body(json!({
                "localId": "uid-123",
                "email": "user@example.com",
                "idToken": "id-token",
                "refreshToken": "refresh-token",
                "expiresIn": "not-a-number"
            }));
        });

        let result = block_on(auth.sign_in_with_email_and_password("user@example.com", "secret"));

        mock.assert();
        assert!(matches!(
            result,
            Err(AuthError::InvalidCredential(message)) if message.contains("Invalid expiresIn value")
        ));
    }

    #[test]
    fn sign_in_propagates_http_errors() {
        let server = start_mock_server();
        let auth = build_auth(&server);

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:signInWithPassword")
                .query_param("key", TEST_API_KEY);
            then.status(400)
                .body("{\"error\":{\"message\":\"INVALID_PASSWORD\"}}");
        });

        let result =
            block_on(auth.sign_in_with_email_and_password("user@example.com", "wrong-password"));

        mock.assert();
        assert!(matches!(
            result,
            Err(AuthError::Network(message)) if message.contains("INVALID_PASSWORD")
        ));
    }

    #[test]
    fn send_password_reset_email_sends_request_body() {
        let server = start_mock_server();
        let auth = build_auth(&server);

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:sendOobCode")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "requestType": "PASSWORD_RESET",
                    "email": TEST_EMAIL
                }));
            then.status(200);
        });

        auth.send_password_reset_email(TEST_EMAIL)
            .expect("password reset should succeed");

        mock.assert();
    }

    #[test]
    fn send_email_verification_uses_current_user_token() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server);

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:sendOobCode")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "requestType": "VERIFY_EMAIL",
                    "idToken": TEST_ID_TOKEN
                }));
            then.status(200);
        });

        auth.send_email_verification()
            .expect("email verification should succeed");

        mock.assert();
    }

    #[test]
    fn confirm_password_reset_posts_new_password() {
        let server = start_mock_server();
        let auth = build_auth(&server);

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:resetPassword")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "oobCode": "reset-code",
                    "newPassword": "new-secret"
                }));
            then.status(200);
        });

        auth.confirm_password_reset("reset-code", "new-secret")
            .expect("confirm reset should succeed");

        mock.assert();
    }

    #[test]
    fn update_profile_sets_display_name() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server);

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:update")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "idToken": TEST_ID_TOKEN,
                    "displayName": "New Name",
                    "returnSecureToken": true
                }));
            then.status(200).json_body(json!({
                "idToken": TEST_ID_TOKEN,
                "refreshToken": TEST_REFRESH_TOKEN,
                "expiresIn": "3600",
                "localId": TEST_UID,
                "email": TEST_EMAIL,
                "displayName": "New Name"
            }));
        });

        let user = auth
            .update_profile(Some("New Name"), None)
            .expect("update profile should succeed");

        mock.assert();
        assert_eq!(user.info().display_name.as_deref(), Some("New Name"));
        assert_eq!(
            user.token_manager().access_token(),
            Some(TEST_ID_TOKEN.to_string())
        );
    }

    #[test]
    fn update_profile_clears_display_name_when_empty_string() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server);

        let set_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:update")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "idToken": TEST_ID_TOKEN,
                    "displayName": "Existing",
                    "returnSecureToken": true
                }));
            then.status(200).json_body(json!({
                "idToken": TEST_ID_TOKEN,
                "refreshToken": TEST_REFRESH_TOKEN,
                "expiresIn": "3600",
                "localId": TEST_UID,
                "email": TEST_EMAIL,
                "displayName": "Existing"
            }));
        });

        auth.update_profile(Some("Existing"), None)
            .expect("initial update should succeed");
        set_mock.assert();

        let clear_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:update")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "idToken": TEST_ID_TOKEN,
                    "deleteAttribute": ["DISPLAY_NAME"],
                    "returnSecureToken": true
                }));
            then.status(200).json_body(json!({
                "idToken": TEST_ID_TOKEN,
                "refreshToken": TEST_REFRESH_TOKEN,
                "expiresIn": "3600",
                "localId": TEST_UID,
                "email": TEST_EMAIL,
                "displayName": ""
            }));
        });

        let user = auth
            .update_profile(Some(""), None)
            .expect("clear update should succeed");

        clear_mock.assert();
        assert!(user.info().display_name.is_none());
    }

    #[test]
    fn update_email_sets_new_email() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server);

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:update")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "idToken": TEST_ID_TOKEN,
                    "email": "new@example.com",
                    "returnSecureToken": true
                }));
            then.status(200).json_body(json!({
                "idToken": UPDATED_ID_TOKEN,
                "refreshToken": UPDATED_REFRESH_TOKEN,
                "expiresIn": "3600",
                "localId": TEST_UID,
                "email": "new@example.com"
            }));
        });

        let user = auth
            .update_email("new@example.com")
            .expect("update email should succeed");

        mock.assert();
        assert_eq!(user.info().email.as_deref(), Some("new@example.com"));
        assert_eq!(
            user.token_manager().access_token(),
            Some(UPDATED_ID_TOKEN.to_string())
        );
    }

    #[test]
    fn update_password_refreshes_tokens() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server);

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:update")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "idToken": TEST_ID_TOKEN,
                    "password": "new-secret",
                    "returnSecureToken": true
                }));
            then.status(200).json_body(json!({
                "idToken": UPDATED_ID_TOKEN,
                "refreshToken": UPDATED_REFRESH_TOKEN,
                "expiresIn": "3600",
                "localId": TEST_UID,
                "email": TEST_EMAIL
            }));
        });

        let user = auth
            .update_password("new-secret")
            .expect("update password should succeed");

        mock.assert();
        assert_eq!(user.uid(), TEST_UID);
        assert_eq!(
            user.token_manager().refresh_token(),
            Some(UPDATED_REFRESH_TOKEN.to_string())
        );
    }

    #[test]
    fn delete_user_clears_current_user_state() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server);

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:delete")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "idToken": TEST_ID_TOKEN
                }));
            then.status(200);
        });

        auth.delete_user().expect("delete user should succeed");

        mock.assert();
        assert!(auth.current_user().is_none());
    }

    #[test]
    fn reauthenticate_with_password_updates_current_user() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server);

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:signInWithPassword")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "email": REAUTH_EMAIL,
                    "password": TEST_PASSWORD,
                    "returnSecureToken": true
                }));
            then.status(200).json_body(json!({
                "localId": REAUTH_UID,
                "email": REAUTH_EMAIL,
                "idToken": REAUTH_ID_TOKEN,
                "refreshToken": REAUTH_REFRESH_TOKEN,
                "expiresIn": "3600"
            }));
        });

        let user = auth
            .reauthenticate_with_password(REAUTH_EMAIL, TEST_PASSWORD)
            .expect("reauth should succeed");

        mock.assert();
        assert_eq!(user.uid(), REAUTH_UID);
        assert_eq!(user.info().email.as_deref(), Some(REAUTH_EMAIL));
        assert_eq!(
            user.token_manager().access_token(),
            Some(REAUTH_ID_TOKEN.to_string())
        );
        assert_eq!(
            auth.current_user().expect("current user set").uid(),
            REAUTH_UID
        );
    }

    #[test]
    fn unlink_providers_sends_delete_provider() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server);

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:update")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "idToken": TEST_ID_TOKEN,
                    "deleteProvider": [GOOGLE_PROVIDER_ID],
                    "returnSecureToken": true
                }));
            then.status(200).json_body(json!({
                "idToken": TEST_ID_TOKEN,
                "refreshToken": TEST_REFRESH_TOKEN,
                "expiresIn": "3600",
                "localId": TEST_UID,
                "email": TEST_EMAIL,
                "providerUserInfo": []
            }));
        });

        let user = auth
            .unlink_providers(&[GOOGLE_PROVIDER_ID])
            .expect("unlink should succeed");

        mock.assert();
        assert_eq!(user.uid(), TEST_UID);
        assert_eq!(user.info().provider_id, EmailAuthProvider::PROVIDER_ID);
    }

    #[test]
    fn unlink_providers_propagates_errors() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server);

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:update")
                .query_param("key", TEST_API_KEY);
            then.status(400)
                .body("{\"error\":{\"message\":\"INVALID_PROVIDER_ID\"}}");
        });

        let result = auth.unlink_providers(&[GOOGLE_PROVIDER_ID]);

        mock.assert();
        assert!(matches!(
            result,
            Err(AuthError::InvalidCredential(message)) if message == "INVALID_PROVIDER_ID"
        ));
    }

    #[test]
    fn get_account_info_returns_users() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server);

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:lookup")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "idToken": TEST_ID_TOKEN
                }));
            then.status(200).json_body(json!({
                "users": [
                    {
                        "localId": TEST_UID,
                        "displayName": "my-name",
                        "email": TEST_EMAIL,
                        "providerUserInfo": [
                            {
                                "providerId": GOOGLE_PROVIDER_ID,
                                "email": TEST_EMAIL
                            }
                        ]
                    }
                ]
            }));
        });

        let response = auth
            .get_account_info()
            .expect("get account info should succeed");

        mock.assert();
        assert_eq!(response.users.len(), 1);
        assert_eq!(response.users[0].display_name.as_deref(), Some("my-name"));
        assert_eq!(response.users[0].email.as_deref(), Some(TEST_EMAIL));
        let providers = response.users[0]
            .provider_user_info
            .as_ref()
            .expect("providers present");
        assert_eq!(providers.len(), 1);
        assert_eq!(
            providers[0].provider_id.as_deref(),
            Some(GOOGLE_PROVIDER_ID)
        );
    }

    #[test]
    fn get_account_info_propagates_errors() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server);

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:lookup")
                .query_param("key", TEST_API_KEY);
            then.status(400)
                .body("{\"error\":{\"message\":\"INVALID_ID_TOKEN\"}}");
        });

        let result = auth.get_account_info();

        mock.assert();
        assert!(matches!(
            result,
            Err(AuthError::InvalidCredential(message)) if message == "INVALID_ID_TOKEN"
        ));
    }
}
