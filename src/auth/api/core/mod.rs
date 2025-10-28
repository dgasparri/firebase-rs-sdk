use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use reqwest::Client;
use reqwest::Url;
use serde::Serialize;
use serde_json::Value;

mod account;
mod idp;
mod mfa;
mod phone;
pub mod token;

use crate::app::{AppError, FirebaseApp, LOGGER as APP_LOGGER};
use crate::auth::error::{AuthError, AuthResult};
use crate::auth::model::MfaEnrollmentInfo;
use crate::auth::model::{
    AuthConfig, AuthCredential, AuthStateListeners, EmailAuthProvider, GetAccountInfoResponse,
    SignInWithCustomTokenRequest, SignInWithCustomTokenResponse, SignInWithEmailLinkRequest,
    SignInWithEmailLinkResponse, SignInWithPasswordRequest, SignInWithPasswordResponse,
    SignUpRequest, SignUpResponse, User, UserCredential, UserInfo,
};
use crate::auth::oauth::{
    credential::OAuthCredential, InMemoryRedirectPersistence, OAuthPopupHandler,
    OAuthRedirectHandler, PendingRedirectEvent, RedirectOperation, RedirectPersistence,
};
#[cfg(all(
    feature = "wasm-web",
    feature = "experimental-indexed-db",
    target_arch = "wasm32"
))]
use crate::auth::persistence::IndexedDbPersistence;
use crate::auth::persistence::{
    AuthPersistence, InMemoryPersistence, PersistedAuthState, PersistenceListener,
    PersistenceSubscription,
};
use crate::auth::types::{
    ActionCodeInfo, ActionCodeInfoData, ActionCodeOperation, ActionCodeSettings, ActionCodeUrl,
    ApplicationVerifier, ConfirmationResult, MultiFactorInfo, MultiFactorSession, MultiFactorUser,
};
use crate::auth::{PhoneAuthCredential, PHONE_PROVIDER_ID};
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentContainer, ComponentType};
#[cfg(feature = "firestore")]
use crate::firestore::remote::datastore::TokenProviderArc;
use crate::platform::runtime::{sleep as runtime_sleep, spawn_detached};
use crate::platform::token::{AsyncTokenProvider, TokenError};
use crate::util::PartialObserver;
use account::{
    apply_action_code, confirm_password_reset, delete_account, get_account_info,
    reset_password_info, send_email_verification, send_password_reset_email,
    send_sign_in_link_to_email, update_account, verify_password, UpdateAccountRequest,
    UpdateAccountResponse, UpdateString,
};
use idp::{sign_in_with_idp, SignInWithIdpRequest, SignInWithIdpResponse};
use mfa::{
    finalize_phone_mfa_enrollment, start_phone_mfa_enrollment, withdraw_mfa,
    FinalizePhoneMfaEnrollmentRequest, PhoneEnrollmentInfo, PhoneVerificationInfo,
    StartPhoneMfaEnrollmentRequest, WithdrawMfaRequest,
};
use phone::{
    link_with_phone_number as api_link_with_phone_number, send_phone_verification_code,
    sign_in_with_phone_number as api_sign_in_with_phone_number, verify_phone_number_for_existing,
    PhoneSignInResponse, SendPhoneVerificationCodeRequest, SignInWithPhoneNumberRequest,
};

const DEFAULT_OAUTH_REQUEST_URI: &str = "http://localhost";
const DEFAULT_IDENTITY_TOOLKIT_ENDPOINT: &str = "https://identitytoolkit.googleapis.com/v1";
const CLIENT_TYPE_WEB: &str = "CLIENT_TYPE_WEB";
const RECAPTCHA_ENTERPRISE: &str = "RECAPTCHA_ENTERPRISE";

struct SignInResponsePayload<'a> {
    local_id: &'a str,
    email: Option<&'a str>,
    phone_number: Option<&'a str>,
    id_token: &'a str,
    refresh_token: &'a str,
    expires_in: Option<&'a str>,
    provider_id: Option<&'a str>,
    operation: &'a str,
    anonymous: bool,
}

#[derive(Clone)]
enum PhoneFinalization {
    SignIn,
    Link { id_token: String },
    Reauth { id_token: String },
}

struct PhoneMfaEnrollmentFinalization {
    id_token: String,
    session_info: String,
    display_name: Option<String>,
}

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
    /// Returns a multi-factor helper tied to this auth instance.
    pub fn multi_factor(self: &Arc<Self>) -> MultiFactorUser {
        MultiFactorUser::new(self.clone())
    }

    /// Creates a builder for configuring an `Auth` instance before construction.
    pub fn builder(app: FirebaseApp) -> AuthBuilder {
        AuthBuilder::new(app)
    }

    /// Constructs an `Auth` instance using in-memory persistence.
    pub fn new(app: FirebaseApp) -> AuthResult<Self> {
        #[cfg(all(
            feature = "wasm-web",
            feature = "experimental-indexed-db",
            target_arch = "wasm32"
        ))]
        let persistence: Arc<dyn AuthPersistence + Send + Sync> =
            Arc::new(IndexedDbPersistence::new());

        #[cfg(all(
            feature = "wasm-web",
            not(feature = "experimental-indexed-db"),
            target_arch = "wasm32"
        ))]
        let persistence: Arc<dyn AuthPersistence + Send + Sync> =
            Arc::new(InMemoryPersistence::default());

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
        let payload = SignInResponsePayload {
            local_id: &response.local_id,
            email: Some(&response.email),
            phone_number: None,
            id_token: &response.id_token,
            refresh_token: &response.refresh_token,
            expires_in: Some(&response.expires_in),
            provider_id: Some(EmailAuthProvider::PROVIDER_ID),
            operation: "signIn",
            anonymous: false,
        };

        self.finalize_sign_in(payload)
    }

    /// Creates a new user using email/password credentials.
    pub async fn create_user_with_email_and_password(
        &self,
        email: &str,
        password: &str,
    ) -> AuthResult<UserCredential> {
        let api_key = self.api_key()?;
        let mut request = SignUpRequest::default();
        request.email = Some(email.to_owned());
        request.password = Some(password.to_owned());
        request.return_secure_token = Some(true);

        let response: SignUpResponse = self
            .execute_request("accounts:signUp", &api_key, &request)
            .await?;

        let local_id = response
            .local_id
            .as_deref()
            .ok_or_else(|| AuthError::InvalidCredential("Missing localId".into()))?;
        let id_token = response
            .id_token
            .as_deref()
            .ok_or_else(|| AuthError::InvalidCredential("Missing idToken".into()))?;
        let refresh_token = response
            .refresh_token
            .as_deref()
            .ok_or_else(|| AuthError::InvalidCredential("Missing refreshToken".into()))?;
        let expires_in = response.expires_in.as_deref();
        let response_email = response.email.as_deref().unwrap_or(email);

        let payload = SignInResponsePayload {
            local_id,
            email: Some(response_email),
            phone_number: None,
            id_token,
            refresh_token,
            expires_in,
            provider_id: Some(EmailAuthProvider::PROVIDER_ID),
            operation: "signUp",
            anonymous: false,
        };

        self.finalize_sign_in(payload)
    }

    /// Exchanges a custom authentication token for Firebase credentials.
    ///
    /// # Examples
    /// ```rust,ignore
    /// let credential = auth.sign_in_with_custom_token(my_signed_jwt).await?;
    /// println!("Signed in as {}", credential.user.uid());
    /// ```
    pub async fn sign_in_with_custom_token(&self, token: &str) -> AuthResult<UserCredential> {
        let api_key = self.api_key()?;
        let request = SignInWithCustomTokenRequest {
            token: token.to_owned(),
            return_secure_token: true,
        };

        let response: SignInWithCustomTokenResponse = self
            .execute_request("accounts:signInWithCustomToken", &api_key, &request)
            .await?;

        let local_id = response
            .local_id
            .as_deref()
            .ok_or_else(|| AuthError::InvalidCredential("Missing localId".into()))?;
        let id_token = response
            .id_token
            .as_deref()
            .ok_or_else(|| AuthError::InvalidCredential("Missing idToken".into()))?;
        let refresh_token = response
            .refresh_token
            .as_deref()
            .ok_or_else(|| AuthError::InvalidCredential("Missing refreshToken".into()))?;
        let expires_in = response.expires_in.as_deref();
        let email = response.email.as_deref();
        let operation = if response.is_new_user.unwrap_or(false) {
            "signUp"
        } else {
            "signIn"
        };

        let payload = SignInResponsePayload {
            local_id,
            email,
            phone_number: None,
            id_token,
            refresh_token,
            expires_in,
            provider_id: Some("custom"),
            operation,
            anonymous: false,
        };

        self.finalize_sign_in(payload)
    }

    /// Signs the user in anonymously, creating an anonymous user if needed.
    ///
    /// # Examples
    /// ```rust,ignore
    /// let anon = auth.sign_in_anonymously().await?;
    /// assert!(anon.user.is_anonymous());
    /// ```
    pub async fn sign_in_anonymously(&self) -> AuthResult<UserCredential> {
        if let Some(user) = self.current_user() {
            if user.is_anonymous() {
                return Ok(UserCredential {
                    user,
                    provider_id: Some("anonymous".to_string()),
                    operation_type: Some("signIn".to_string()),
                });
            }
        }

        let api_key = self.api_key()?;
        let mut request = SignUpRequest::default();
        request.return_secure_token = Some(true);

        let response: SignUpResponse = self
            .execute_request("accounts:signUp", &api_key, &request)
            .await?;

        let local_id = response
            .local_id
            .as_deref()
            .ok_or_else(|| AuthError::InvalidCredential("Missing localId".into()))?;
        let id_token = response
            .id_token
            .as_deref()
            .ok_or_else(|| AuthError::InvalidCredential("Missing idToken".into()))?;
        let refresh_token = response
            .refresh_token
            .as_deref()
            .ok_or_else(|| AuthError::InvalidCredential("Missing refreshToken".into()))?;
        let expires_in = response.expires_in.as_deref();

        let payload = SignInResponsePayload {
            local_id,
            email: None,
            phone_number: None,
            id_token,
            refresh_token,
            expires_in,
            provider_id: Some("anonymous"),
            operation: "signIn",
            anonymous: true,
        };

        self.finalize_sign_in(payload)
    }

    /// Starts a phone number sign-in flow, returning a confirmation handle.
    pub async fn sign_in_with_phone_number(
        self: &Arc<Self>,
        phone_number: &str,
        verifier: Arc<dyn ApplicationVerifier>,
    ) -> AuthResult<ConfirmationResult> {
        self.start_phone_flow(phone_number, verifier, PhoneFinalization::SignIn)
            .await
    }

    /// Sends an SMS verification code and returns the verification identifier.
    ///
    /// # Examples
    /// ```rust,ignore
    /// let verification_id = auth
    ///     .send_phone_verification_code("+15551234567", verifier)
    ///     .await?;
    /// ```
    pub async fn send_phone_verification_code(
        &self,
        phone_number: &str,
        verifier: Arc<dyn ApplicationVerifier>,
    ) -> AuthResult<String> {
        self.send_phone_verification(phone_number, verifier).await
    }

    /// Links the currently signed-in account with the provided phone number.
    pub async fn link_with_phone_number(
        self: &Arc<Self>,
        phone_number: &str,
        verifier: Arc<dyn ApplicationVerifier>,
    ) -> AuthResult<ConfirmationResult> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        self.start_phone_flow(phone_number, verifier, PhoneFinalization::Link { id_token })
            .await
    }

    /// Reauthenticates the current user via an SMS verification code.
    pub async fn reauthenticate_with_phone_number(
        self: &Arc<Self>,
        phone_number: &str,
        verifier: Arc<dyn ApplicationVerifier>,
    ) -> AuthResult<ConfirmationResult> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        self.start_phone_flow(
            phone_number,
            verifier,
            PhoneFinalization::Reauth { id_token },
        )
        .await
    }

    /// Signs in using a credential produced by [`PhoneAuthProvider::credential`].
    ///
    /// # Examples
    /// ```rust,ignore
    /// let credential = PhoneAuthProvider::credential(verification_id, sms_code);
    /// let user = auth.sign_in_with_phone_credential(credential).await?;
    /// ```
    pub async fn sign_in_with_phone_credential(
        self: &Arc<Self>,
        credential: PhoneAuthCredential,
    ) -> AuthResult<UserCredential> {
        self.finalize_phone_credential(credential, PhoneFinalization::SignIn)
            .await
    }

    /// Links the current user with the provided phone credential.
    pub async fn link_with_phone_credential(
        self: &Arc<Self>,
        credential: PhoneAuthCredential,
    ) -> AuthResult<UserCredential> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        self.finalize_phone_credential(credential, PhoneFinalization::Link { id_token })
            .await
    }

    /// Reauthenticates the current user with an SMS credential.
    ///
    /// # Examples
    /// ```rust,ignore
    /// let credential = PhoneAuthProvider::credential(verification_id, sms_code);
    /// auth.reauthenticate_with_phone_credential(credential).await?;
    /// ```
    pub async fn reauthenticate_with_phone_credential(
        self: &Arc<Self>,
        credential: PhoneAuthCredential,
    ) -> AuthResult<Arc<User>> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        let result = self
            .finalize_phone_credential(credential, PhoneFinalization::Reauth { id_token })
            .await?;
        Ok(result.user)
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

    async fn start_phone_flow(
        self: &Arc<Self>,
        phone_number: &str,
        verifier: Arc<dyn ApplicationVerifier>,
        flow: PhoneFinalization,
    ) -> AuthResult<ConfirmationResult> {
        let verification_id = self
            .send_phone_verification(phone_number, Arc::clone(&verifier))
            .await?;
        let session_info = Arc::new(verification_id.clone());
        let auth = Arc::clone(self);
        let flow_holder = Arc::new(flow);

        Ok(ConfirmationResult::new(
            verification_id,
            move |code: &str| {
                let auth = Arc::clone(&auth);
                let session = Arc::clone(&session_info);
                let flow = Arc::clone(&flow_holder);
                let code = code.to_owned();
                async move {
                    Auth::finalize_phone_confirmation(
                        auth,
                        (*session).clone(),
                        code,
                        (*flow).clone(),
                    )
                    .await
                }
            },
        ))
    }

    pub(crate) async fn send_phone_verification(
        &self,
        phone_number: &str,
        verifier: Arc<dyn ApplicationVerifier>,
    ) -> AuthResult<String> {
        let api_key = self.api_key()?;
        let endpoint = self.identity_toolkit_endpoint();
        let request = self.build_phone_verification_request(phone_number, verifier)?;
        let response =
            send_phone_verification_code(&self.rest_client, &endpoint, &api_key, &request).await?;
        Ok(response.session_info)
    }

    fn build_phone_verification_request(
        &self,
        phone_number: &str,
        verifier: Arc<dyn ApplicationVerifier>,
    ) -> AuthResult<SendPhoneVerificationCodeRequest> {
        let token = verifier.verify()?;
        let verifier_type = verifier.verifier_type().to_lowercase();
        let mut request = SendPhoneVerificationCodeRequest {
            phone_number: phone_number.to_string(),
            ..Default::default()
        };

        match verifier_type.as_str() {
            "recaptcha" | "recaptcha-v2" => {
                request.recaptcha_token = Some(token);
            }
            "recaptcha-enterprise" => {
                request.captcha_response = Some(token);
                request.client_type = Some(CLIENT_TYPE_WEB.to_string());
                request.recaptcha_version = Some(RECAPTCHA_ENTERPRISE.to_string());
            }
            other => {
                request.captcha_response = Some(token);
                request.client_type = Some(other.to_string());
            }
        }

        if request.client_type.is_none() {
            request.client_type = Some(CLIENT_TYPE_WEB.to_string());
        }

        Ok(request)
    }

    fn build_phone_enrollment_info(
        &self,
        phone_number: &str,
        verifier: Arc<dyn ApplicationVerifier>,
    ) -> AuthResult<PhoneEnrollmentInfo> {
        let token = verifier.verify()?;
        let verifier_type = verifier.verifier_type().to_lowercase();
        let mut info = PhoneEnrollmentInfo {
            phone_number: phone_number.to_string(),
            recaptcha_token: None,
            captcha_response: None,
            client_type: Some(CLIENT_TYPE_WEB.to_string()),
            recaptcha_version: None,
        };

        match verifier_type.as_str() {
            "recaptcha" | "recaptcha-v2" => {
                info.recaptcha_token = Some(token);
            }
            "recaptcha-enterprise" => {
                info.captcha_response = Some(token);
                info.recaptcha_version = Some(RECAPTCHA_ENTERPRISE.to_string());
            }
            other => {
                info.captcha_response = Some(token);
                info.client_type = Some(other.to_string());
            }
        }

        if info.client_type.is_none() {
            info.client_type = Some(CLIENT_TYPE_WEB.to_string());
        }

        Ok(info)
    }

    pub(crate) async fn start_phone_mfa_enrollment(
        self: &Arc<Self>,
        phone_number: &str,
        verifier: Arc<dyn ApplicationVerifier>,
        display_name: Option<&str>,
    ) -> AuthResult<ConfirmationResult> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        let api_key = self.api_key()?;
        let endpoint = self.identity_toolkit_endpoint();
        let enrollment_info = self.build_phone_enrollment_info(phone_number, verifier)?;
        let request = StartPhoneMfaEnrollmentRequest {
            id_token: id_token.clone(),
            phone_enrollment_info: enrollment_info,
            tenant_id: None,
        };

        let response =
            start_phone_mfa_enrollment(&self.rest_client, &endpoint, &api_key, &request).await?;

        let session_info = response.phone_session_info.session_info;
        let auth = Arc::clone(self);
        let display_name = display_name.map(|value| value.to_string());
        let id_token_for_flow = id_token.clone();
        Ok(ConfirmationResult::new(
            session_info.clone(),
            move |code: &str| {
                let auth = Arc::clone(&auth);
                let code = code.to_string();
                let session = session_info.clone();
                let display = display_name.clone();
                let id_token_value = id_token_for_flow.clone();
                async move {
                    auth.complete_phone_mfa_enrollment(
                        PhoneMfaEnrollmentFinalization {
                            id_token: id_token_value,
                            session_info: session,
                            display_name: display,
                        },
                        code,
                    )
                    .await
                }
            },
        ))
    }

    async fn complete_phone_mfa_enrollment(
        self: Arc<Self>,
        flow: PhoneMfaEnrollmentFinalization,
        verification_code: String,
    ) -> AuthResult<UserCredential> {
        let api_key = self.api_key()?;
        let endpoint = self.identity_toolkit_endpoint();
        let request = FinalizePhoneMfaEnrollmentRequest {
            id_token: flow.id_token.clone(),
            phone_verification_info: PhoneVerificationInfo {
                session_info: flow.session_info.clone(),
                code: verification_code,
            },
            display_name: flow.display_name.clone(),
            tenant_id: None,
        };

        let response =
            finalize_phone_mfa_enrollment(&self.rest_client, &endpoint, &api_key, &request).await?;

        self.update_current_user_tokens(
            response.id_token.clone(),
            response.refresh_token.clone(),
            None,
        )?;
        let user = self.refresh_current_user_profile().await?;

        Ok(UserCredential {
            user,
            provider_id: Some(PHONE_PROVIDER_ID.to_string()),
            operation_type: Some("enroll".to_string()),
        })
    }

    pub(crate) async fn withdraw_multi_factor(&self, enrollment_id: &str) -> AuthResult<()> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        let api_key = self.api_key()?;
        let endpoint = self.identity_toolkit_endpoint();
        let request = WithdrawMfaRequest {
            id_token: id_token.clone(),
            mfa_enrollment_id: enrollment_id.to_string(),
            tenant_id: None,
        };

        let response = withdraw_mfa(&self.rest_client, &endpoint, &api_key, &request).await?;
        let new_id_token = response.id_token.unwrap_or(id_token);
        let new_refresh_token = response
            .refresh_token
            .or_else(|| user.refresh_token())
            .unwrap_or_default();

        self.update_current_user_tokens(new_id_token, new_refresh_token, None)?;
        self.refresh_current_user_profile().await?;
        Ok(())
    }

    fn endpoint_url(&self, path: &str, api_key: &str) -> AuthResult<Url> {
        let base = self.identity_toolkit_endpoint();
        let endpoint = format!("{}/{}?key={}", base.trim_end_matches('/'), path, api_key);
        Url::parse(&endpoint).map_err(|err| AuthError::Network(err.to_string()))
    }

    fn build_user(
        &self,
        local_id: &str,
        email: Option<&str>,
        phone_number: Option<&str>,
        provider_id: Option<&str>,
    ) -> User {
        let info = UserInfo {
            uid: local_id.to_string(),
            display_name: None,
            email: email.map(|value| value.to_string()),
            phone_number: phone_number.map(|value| value.to_string()),
            photo_url: None,
            provider_id: provider_id
                .unwrap_or(EmailAuthProvider::PROVIDER_ID)
                .to_string(),
        };
        User::new(self.app.clone(), info)
    }

    fn finalize_sign_in(&self, payload: SignInResponsePayload<'_>) -> AuthResult<UserCredential> {
        let SignInResponsePayload {
            local_id,
            email,
            phone_number,
            id_token,
            refresh_token,
            expires_in,
            provider_id,
            operation,
            anonymous,
        } = payload;

        let mut user = self.build_user(local_id, email, phone_number, provider_id);
        user.set_anonymous(anonymous);
        let expiration = expires_in
            .map(|value| self.parse_expires_in(value))
            .transpose()?;
        user.update_tokens(
            Some(id_token.to_string()),
            Some(refresh_token.to_string()),
            expiration,
        );
        let user_arc = Arc::new(user);
        *self.current_user.lock().unwrap() = Some(user_arc.clone());
        self.after_token_update(user_arc.clone())?;
        self.listeners.notify(user_arc.clone());

        Ok(UserCredential {
            user: user_arc,
            provider_id: provider_id.map(|id| id.to_string()),
            operation_type: Some(operation.to_string()),
        })
    }

    pub(crate) async fn fetch_enrolled_factors(&self) -> AuthResult<Vec<MultiFactorInfo>> {
        let user = self.refresh_current_user_profile().await?;
        Ok(user.mfa_info())
    }

    pub(crate) async fn multi_factor_session(&self) -> AuthResult<MultiFactorSession> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        Ok(MultiFactorSession { id_token })
    }

    fn update_current_user_tokens(
        &self,
        id_token: String,
        refresh_token: String,
        expires_in: Option<Duration>,
    ) -> AuthResult<Arc<User>> {
        let user = self.require_current_user()?;
        user.update_tokens(Some(id_token), Some(refresh_token), expires_in);
        self.after_token_update(user.clone())?;
        Ok(user)
    }

    async fn refresh_current_user_profile(&self) -> AuthResult<Arc<User>> {
        let Some(current) = self.current_user() else {
            return Err(AuthError::InvalidCredential("No user signed in".into()));
        };

        let info = self.get_account_info().await?;
        let account = info
            .users
            .into_iter()
            .find(|user| user.local_id.as_deref() == Some(current.uid()))
            .ok_or_else(|| {
                AuthError::InvalidCredential("Account info missing current user".into())
            })?;

        let provider_id = account
            .provider_user_info
            .as_ref()
            .and_then(|infos| infos.first())
            .and_then(|info| info.provider_id.clone())
            .unwrap_or_else(|| current.info().provider_id.clone());

        let info = UserInfo {
            uid: account
                .local_id
                .clone()
                .unwrap_or_else(|| current.uid().to_string()),
            display_name: account
                .display_name
                .clone()
                .or_else(|| current.info().display_name.clone()),
            email: account
                .email
                .clone()
                .or_else(|| current.info().email.clone()),
            phone_number: account
                .phone_number
                .clone()
                .or_else(|| current.info().phone_number.clone()),
            photo_url: account
                .photo_url
                .clone()
                .or_else(|| current.info().photo_url.clone()),
            provider_id,
        };

        let mut new_user = User::new(self.app.clone(), info);
        new_user.set_email_verified(account.email_verified.unwrap_or(current.email_verified()));
        new_user.set_anonymous(current.is_anonymous());
        let access_token = current.token_manager().access_token();
        let refresh_token = current.refresh_token();
        let expiration = current.token_manager().expiration_time();
        new_user
            .token_manager()
            .initialize(access_token, refresh_token, expiration);

        if let Some(entries) = account.mfa_info.as_ref() {
            let factors = Self::convert_mfa_entries(entries);
            new_user.set_mfa_info(factors);
        }

        let new_user = Arc::new(new_user);
        *self.current_user.lock().unwrap() = Some(new_user.clone());
        self.after_token_update(new_user.clone())?;
        self.listeners.notify(new_user.clone());
        Ok(new_user)
    }

    fn convert_mfa_entries(entries: &[MfaEnrollmentInfo]) -> Vec<MultiFactorInfo> {
        entries
            .iter()
            .filter_map(MultiFactorInfo::from_enrollment)
            .collect()
    }

    async fn finalize_phone_confirmation(
        self: Arc<Self>,
        session_info: String,
        verification_code: String,
        flow: PhoneFinalization,
    ) -> AuthResult<UserCredential> {
        let api_key = self.api_key()?;
        let endpoint = self.identity_toolkit_endpoint();
        let mut request = SignInWithPhoneNumberRequest::default();
        request.session_info = Some(session_info);
        request.code = Some(verification_code);

        let response = match &flow {
            PhoneFinalization::SignIn => {
                api_sign_in_with_phone_number(&self.rest_client, &endpoint, &api_key, &request)
                    .await?
            }
            PhoneFinalization::Link { id_token } => {
                request.id_token = Some(id_token.clone());
                api_link_with_phone_number(&self.rest_client, &endpoint, &api_key, &request).await?
            }
            PhoneFinalization::Reauth { id_token } => {
                request.id_token = Some(id_token.clone());
                verify_phone_number_for_existing(&self.rest_client, &endpoint, &api_key, &request)
                    .await?
            }
        };

        self.handle_phone_response(response, flow).await
    }

    async fn finalize_phone_credential(
        self: &Arc<Self>,
        credential: PhoneAuthCredential,
        flow: PhoneFinalization,
    ) -> AuthResult<UserCredential> {
        let (verification_id, verification_code) = credential.into_parts();
        self.clone()
            .finalize_phone_confirmation(verification_id, verification_code, flow)
            .await
    }

    async fn handle_phone_response(
        &self,
        response: PhoneSignInResponse,
        flow: PhoneFinalization,
    ) -> AuthResult<UserCredential> {
        let local_id = response
            .local_id
            .ok_or_else(|| AuthError::InvalidCredential("Missing localId".into()))?;
        let id_token = response
            .id_token
            .ok_or_else(|| AuthError::InvalidCredential("Missing idToken".into()))?;
        let refresh_token = response
            .refresh_token
            .ok_or_else(|| AuthError::InvalidCredential("Missing refreshToken".into()))?;
        let expires_in = response.expires_in;
        let phone_number = response.phone_number;
        let is_new_user = response.is_new_user.unwrap_or(false);

        let operation = match flow {
            PhoneFinalization::SignIn => {
                if is_new_user {
                    "signUp"
                } else {
                    "signIn"
                }
            }
            PhoneFinalization::Link { .. } => "link",
            PhoneFinalization::Reauth { .. } => "reauthenticate",
        };

        let payload = SignInResponsePayload {
            local_id: local_id.as_str(),
            email: None,
            phone_number: phone_number.as_deref(),
            id_token: id_token.as_str(),
            refresh_token: refresh_token.as_str(),
            expires_in: expires_in.as_deref(),
            provider_id: Some(PHONE_PROVIDER_ID),
            operation,
            anonymous: false,
        };

        let credential = self.finalize_sign_in(payload)?;
        self.refresh_current_user_profile().await?;
        Ok(credential)
    }

    fn map_mfa_info(entries: &[MfaEnrollmentInfo]) -> Option<MultiFactorInfo> {
        entries
            .iter()
            .filter_map(MultiFactorInfo::from_enrollment)
            .next()
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
    pub async fn sign_in_with_oauth_credential(
        &self,
        credential: AuthCredential,
    ) -> AuthResult<UserCredential> {
        self.exchange_oauth_credential(credential, None).await
    }

    /// Sends a password reset email to the specified address.
    pub async fn send_password_reset_email(&self, email: &str) -> AuthResult<()> {
        let api_key = self.api_key()?;
        let endpoint = self.identity_toolkit_endpoint();
        send_password_reset_email(&self.rest_client, &endpoint, &api_key, email).await
    }

    /// Sends a sign-in link to the provided email address.
    ///
    /// # Examples
    /// ```rust,ignore
    /// let settings = ActionCodeSettings {
    ///     url: "https://example.com/finish".into(),
    ///     handle_code_in_app: true,
    ///     ..Default::default()
    /// };
    /// auth.send_sign_in_link_to_email("user@example.com", &settings).await?;
    /// ```
    pub async fn send_sign_in_link_to_email(
        &self,
        email: &str,
        settings: &ActionCodeSettings,
    ) -> AuthResult<()> {
        let api_key = self.api_key()?;
        let endpoint = self.identity_toolkit_endpoint();
        send_sign_in_link_to_email(&self.rest_client, &endpoint, &api_key, email, settings).await
    }

    /// Confirms a password reset OOB code and applies the new password.
    pub async fn confirm_password_reset(
        &self,
        oob_code: &str,
        new_password: &str,
    ) -> AuthResult<()> {
        let api_key = self.api_key()?;
        let endpoint = self.identity_toolkit_endpoint();
        confirm_password_reset(
            &self.rest_client,
            &endpoint,
            &api_key,
            oob_code,
            new_password,
        )
        .await
    }

    /// Sends an email verification message to the currently signed-in user.
    pub async fn send_email_verification(&self) -> AuthResult<()> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        let api_key = self.api_key()?;
        let endpoint = self.identity_toolkit_endpoint();
        send_email_verification(&self.rest_client, &endpoint, &api_key, &id_token).await
    }

    /// Returns `true` if the supplied link is an email sign-in link.
    pub fn is_sign_in_with_email_link(&self, email_link: &str) -> bool {
        ActionCodeUrl::parse(email_link)
            .map(|url| url.operation == ActionCodeOperation::EmailSignIn)
            .unwrap_or(false)
    }

    /// Completes the email link sign-in flow for the given email address.
    ///
    /// # Examples
    /// ```rust,ignore
    /// if auth.is_sign_in_with_email_link(&link) {
    ///     let credential = auth.sign_in_with_email_link("user@example.com", &link).await?;
    ///     println!("Signed in as {}", credential.user.uid());
    /// }
    /// ```
    pub async fn sign_in_with_email_link(
        &self,
        email: &str,
        email_link: &str,
    ) -> AuthResult<UserCredential> {
        let api_key = self.api_key()?;
        let action_url = ActionCodeUrl::parse(email_link)
            .ok_or_else(|| AuthError::InvalidCredential("Invalid email action link".into()))?;

        if action_url.operation != ActionCodeOperation::EmailSignIn {
            return Err(AuthError::InvalidCredential(
                "Action link does not represent an email sign-in operation".into(),
            ));
        }

        let request = SignInWithEmailLinkRequest {
            email: email.to_owned(),
            oob_code: action_url.code.clone(),
            return_secure_token: true,
            tenant_id: action_url.tenant_id.clone(),
            id_token: None,
        };

        let response: SignInWithEmailLinkResponse = self
            .execute_request("accounts:signInWithEmailLink", &api_key, &request)
            .await?;

        let local_id = response
            .local_id
            .as_deref()
            .ok_or_else(|| AuthError::InvalidCredential("Missing localId".into()))?;
        let id_token = response
            .id_token
            .as_deref()
            .ok_or_else(|| AuthError::InvalidCredential("Missing idToken".into()))?;
        let refresh_token = response
            .refresh_token
            .as_deref()
            .ok_or_else(|| AuthError::InvalidCredential("Missing refreshToken".into()))?;
        let expires_in = response.expires_in.as_deref();
        let response_email = response.email.as_deref().unwrap_or(email);
        let operation = if response.is_new_user.unwrap_or(false) {
            "signUp"
        } else {
            "signIn"
        };

        let payload = SignInResponsePayload {
            local_id,
            email: Some(response_email),
            phone_number: None,
            id_token,
            refresh_token,
            expires_in,
            provider_id: Some(EmailAuthProvider::PROVIDER_ID),
            operation,
            anonymous: false,
        };

        self.finalize_sign_in(payload)
    }

    /// Applies an out-of-band action code issued by Firebase Auth.
    ///
    /// # Examples
    /// ```rust,ignore
    /// auth.apply_action_code("ACTION_CODE_FROM_EMAIL").await?;
    /// ```
    pub async fn apply_action_code(&self, oob_code: &str) -> AuthResult<()> {
        let api_key = self.api_key()?;
        let endpoint = self.identity_toolkit_endpoint();
        apply_action_code(&self.rest_client, &endpoint, &api_key, oob_code, None).await
    }

    /// Retrieves metadata describing the provided action code.
    ///
    /// # Examples
    /// ```rust,ignore
    /// let info = auth.check_action_code("ACTION_CODE").await?;
    /// println!("Operation: {:?}", info.operation);
    /// ```
    pub async fn check_action_code(&self, oob_code: &str) -> AuthResult<ActionCodeInfo> {
        let api_key = self.api_key()?;
        let endpoint = self.identity_toolkit_endpoint();
        let response =
            reset_password_info(&self.rest_client, &endpoint, &api_key, oob_code, None).await?;

        let request_type = response
            .request_type
            .as_deref()
            .ok_or_else(|| AuthError::InvalidCredential("Missing requestType".into()))?;
        let operation = ActionCodeOperation::from_request_type(request_type).ok_or_else(|| {
            AuthError::InvalidCredential(format!("Unknown requestType: {request_type}"))
        })?;

        let email = if operation == ActionCodeOperation::VerifyAndChangeEmail {
            response.new_email.as_deref()
        } else {
            response.email.as_deref()
        };
        let previous_email = if operation == ActionCodeOperation::VerifyAndChangeEmail {
            response.email.as_deref()
        } else {
            response.new_email.as_deref()
        };

        let multi_factor_info = response
            .mfa_info
            .as_ref()
            .and_then(|infos| Self::map_mfa_info(infos));

        let data = ActionCodeInfoData {
            email: email.map(|value| value.to_string()),
            previous_email: previous_email.map(|value| value.to_string()),
            multi_factor_info,
            from_email: None,
        };

        Ok(ActionCodeInfo { data, operation })
    }

    /// Returns the email address associated with the provided password reset code.
    ///
    /// # Examples
    /// ```rust,ignore
    /// let email = auth.verify_password_reset_code("RESET_CODE").await?;
    /// println!("Reset applies to {email}");
    /// ```
    pub async fn verify_password_reset_code(&self, oob_code: &str) -> AuthResult<String> {
        let info = self.check_action_code(oob_code).await?;
        info.data
            .email
            .clone()
            .ok_or_else(|| AuthError::InvalidCredential("Action code missing email".into()))
    }

    /// Updates the current user's display name and photo URL.
    pub async fn update_profile(
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

        self.perform_account_update(user, request).await
    }

    /// Updates the current user's email address.
    pub async fn update_email(&self, email: &str) -> AuthResult<Arc<User>> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        let mut request = UpdateAccountRequest::new(id_token);
        request.email = Some(email.to_string());
        self.perform_account_update(user, request).await
    }

    /// Updates the current user's password.
    pub async fn update_password(&self, password: &str) -> AuthResult<Arc<User>> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        let mut request = UpdateAccountRequest::new(id_token);
        request.password = Some(password.to_string());
        self.perform_account_update(user, request).await
    }

    /// Deletes the current user from Firebase Auth.
    pub async fn delete_user(&self) -> AuthResult<()> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        let api_key = self.api_key()?;
        let endpoint = self.identity_toolkit_endpoint();
        delete_account(&self.rest_client, &endpoint, &api_key, &id_token).await?;
        self.sign_out();
        Ok(())
    }

    /// Unlinks the specified providers from the current user.
    pub async fn unlink_providers(&self, provider_ids: &[&str]) -> AuthResult<Arc<User>> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        let mut request = UpdateAccountRequest::new(id_token);
        request.delete_providers = provider_ids.iter().map(|id| id.to_string()).collect();
        self.perform_account_update(user, request).await
    }

    /// Fetches the latest account info for the current user.
    pub async fn get_account_info(&self) -> AuthResult<GetAccountInfoResponse> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        let api_key = self.api_key()?;
        let endpoint = self.identity_toolkit_endpoint();
        get_account_info(&self.rest_client, &endpoint, &api_key, &id_token).await
    }

    /// Links an OAuth credential with the currently signed-in user.
    pub async fn link_with_oauth_credential(
        &self,
        credential: AuthCredential,
    ) -> AuthResult<UserCredential> {
        let user = self.require_current_user()?;
        let id_token = user.get_id_token(false)?;
        self.exchange_oauth_credential(credential, Some(id_token))
            .await
    }

    /// Reauthenticates the current user with email and password.
    pub async fn reauthenticate_with_password(
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
        let response = verify_password(&self.rest_client, &endpoint, &api_key, &request).await?;
        self.apply_password_reauth(response)
    }

    /// Reauthenticates the current user with an OAuth credential.
    pub async fn reauthenticate_with_oauth_credential(
        &self,
        credential: AuthCredential,
    ) -> AuthResult<Arc<User>> {
        let user = self.require_current_user()?;
        let result = self
            .exchange_oauth_credential(credential, Some(user.get_id_token(false)?))
            .await?;
        Ok(result.user)
    }

    async fn exchange_oauth_credential(
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
        let response = sign_in_with_idp(&self.rest_client, &api_key, &request).await?;
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

    async fn perform_account_update(
        &self,
        current_user: Arc<User>,
        request: UpdateAccountRequest,
    ) -> AuthResult<Arc<User>> {
        let api_key = self.api_key()?;
        let endpoint = self.identity_toolkit_endpoint();
        let response = update_account(&self.rest_client, &endpoint, &api_key, &request).await?;
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
        let mut user = self.build_user(
            &response.local_id,
            Some(&response.email),
            None,
            Some(EmailAuthProvider::PROVIDER_ID),
        );
        user.set_anonymous(false);
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

        let mut user = User::new(self.app.clone(), info);
        user.set_email_verified(current_user.email_verified());
        if let Some(entries) = response.mfa_info.as_ref() {
            let factors = Self::convert_mfa_entries(entries);
            user.set_mfa_info(factors);
        }
        user.update_tokens(Some(id_token), Some(refresh_token), expires_in);

        let user_arc = Arc::new(user);
        *self.current_user.lock().unwrap() = Some(user_arc.clone());
        self.after_token_update(user_arc.clone())?;
        Ok(user_arc)
    }

    fn schedule_refresh_for_user(&self, user: Arc<User>) {
        self.cancel_scheduled_refresh();

        let Some(expiration) = user.token_manager().expiration_time() else {
            return;
        };

        let trigger_time = expiration
            .checked_sub(self.token_refresh_tolerance)
            .unwrap_or(expiration);
        let now = SystemTime::now();
        let delay = trigger_time
            .duration_since(now)
            .unwrap_or(Duration::from_secs(0));

        let cancel_flag = Arc::new(AtomicBool::new(false));
        *self.refresh_cancel.lock().unwrap() = Some(cancel_flag.clone());

        let Some(auth_arc) = self.self_arc() else {
            return;
        };

        let user_for_refresh = user.clone();
        spawn_detached(async move {
            if !delay.is_zero() {
                runtime_sleep(delay).await;
            }

            if cancel_flag.load(Ordering::SeqCst) {
                return;
            }

            if let Err(err) = auth_arc.refresh_user_token(&user_for_refresh).await {
                APP_LOGGER.warn(format!("Failed to refresh Auth token: {err}"));
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
    use crate::auth::types::{
        ActionCodeSettings, AndroidSettings, ApplicationVerifier, IosSettings,
    };
    use crate::auth::PhoneAuthProvider;
    use crate::test_support::{start_mock_server, test_firebase_app_with_api_key};
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

    struct StaticVerifier {
        token: &'static str,
        kind: &'static str,
    }

    impl ApplicationVerifier for StaticVerifier {
        fn verify(&self) -> AuthResult<String> {
            Ok(self.token.to_string())
        }

        fn verifier_type(&self) -> &str {
            self.kind
        }
    }

    fn build_auth(server: &MockServer) -> Arc<Auth> {
        Auth::builder(test_firebase_app_with_api_key(TEST_API_KEY))
            .with_identity_toolkit_endpoint(server.url("/v1"))
            .with_secure_token_endpoint(server.url("/token"))
            .defer_initialization()
            .build()
            .expect("failed to build auth")
    }

    async fn sign_in_user(auth: &Arc<Auth>, server: &MockServer) {
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

        auth.sign_in_with_email_and_password(TEST_EMAIL, TEST_PASSWORD)
            .await
            .expect("sign-in should succeed");
        mock.assert();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn sign_in_with_email_and_password_success() {
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

        let credential = auth
            .sign_in_with_email_and_password("user@example.com", "secret")
            .await
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

    #[tokio::test(flavor = "current_thread")]
    async fn create_user_with_email_and_password_success() {
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

        let credential = auth
            .create_user_with_email_and_password("user@example.com", "secret")
            .await
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

    #[tokio::test(flavor = "current_thread")]
    async fn sign_in_with_invalid_expires_in_returns_error() {
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

        let result = auth
            .sign_in_with_email_and_password("user@example.com", "secret")
            .await;

        mock.assert();
        assert!(matches!(
            result,
            Err(AuthError::InvalidCredential(message)) if message.contains("Invalid expiresIn value")
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn sign_in_propagates_http_errors() {
        let server = start_mock_server();
        let auth = build_auth(&server);

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:signInWithPassword")
                .query_param("key", TEST_API_KEY);
            then.status(400)
                .body("{\"error\":{\"message\":\"INVALID_PASSWORD\"}}");
        });

        let result = auth
            .sign_in_with_email_and_password("user@example.com", "wrong-password")
            .await;

        mock.assert();
        assert!(matches!(
            result,
            Err(AuthError::Network(message)) if message.contains("INVALID_PASSWORD")
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn send_password_reset_email_sends_request_body() {
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
            .await
            .expect("password reset should succeed");

        mock.assert();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn send_sign_in_link_to_email_posts_expected_body() {
        let server = start_mock_server();
        let auth = build_auth(&server);

        let settings = ActionCodeSettings {
            url: "https://example.com/finish".into(),
            handle_code_in_app: true,
            i_os: Some(IosSettings {
                bundle_id: "com.example.ios".into(),
            }),
            android: Some(AndroidSettings {
                package_name: "com.example.android".into(),
                install_app: Some(true),
                minimum_version: Some("12".into()),
            }),
            dynamic_link_domain: Some("example.page.link".into()),
            link_domain: Some("example.firebaseapp.com".into()),
        };

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:sendOobCode")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "requestType": "EMAIL_SIGNIN",
                    "email": TEST_EMAIL,
                    "continueUrl": "https://example.com/finish",
                    "dynamicLinkDomain": "example.page.link",
                    "linkDomain": "example.firebaseapp.com",
                    "canHandleCodeInApp": true,
                    "clientType": "CLIENT_TYPE_WEB",
                    "iOSBundleId": "com.example.ios",
                    "androidPackageName": "com.example.android",
                    "androidInstallApp": true,
                    "androidMinimumVersionCode": "12"
                }));
            then.status(200);
        });

        auth.send_sign_in_link_to_email(TEST_EMAIL, &settings)
            .await
            .expect("sign-in link should be sent");

        mock.assert();
    }

    #[test]
    fn is_sign_in_with_email_link_checks_operation() {
        let server = start_mock_server();
        let auth = build_auth(&server);

        let valid_link = format!(
            "https://example.com/action?apiKey={}&oobCode=oob-code&mode=signIn",
            TEST_API_KEY
        );
        assert!(auth.is_sign_in_with_email_link(&valid_link));

        let invalid_link = format!(
            "https://example.com/action?apiKey={}&oobCode=oob-code&mode=verifyEmail",
            TEST_API_KEY
        );
        assert!(!auth.is_sign_in_with_email_link(&invalid_link));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn sign_in_with_email_link_success() {
        let server = start_mock_server();
        let auth = build_auth(&server);

        let email_link = format!(
            "https://example.com/action?apiKey={}&oobCode=oob-code&mode=signIn",
            TEST_API_KEY
        );

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:signInWithEmailLink")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "email": TEST_EMAIL,
                    "oobCode": "oob-code",
                    "returnSecureToken": true
                }));
            then.status(200).json_body(json!({
                "localId": "email-link-uid",
                "email": TEST_EMAIL,
                "idToken": "email-link-id-token",
                "refreshToken": "email-link-refresh",
                "expiresIn": "3600"
            }));
        });

        let credential = auth
            .sign_in_with_email_link(TEST_EMAIL, &email_link)
            .await
            .expect("email link sign-in should succeed");

        mock.assert();
        assert_eq!(credential.user.uid(), "email-link-uid");
        assert_eq!(
            credential.provider_id.as_deref(),
            Some(EmailAuthProvider::PROVIDER_ID)
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn apply_action_code_posts_oob_code() {
        let server = start_mock_server();
        let auth = build_auth(&server);

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:update")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "oobCode": "action-code"
                }));
            then.status(200);
        });

        auth.apply_action_code("action-code")
            .await
            .expect("apply action code should succeed");

        mock.assert();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn check_action_code_returns_info() {
        let server = start_mock_server();
        let auth = build_auth(&server);

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:resetPassword")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "oobCode": "reset-code"
                }));
            then.status(200).json_body(json!({
                "email": TEST_EMAIL,
                "requestType": "PASSWORD_RESET"
            }));
        });

        let info = auth
            .check_action_code("reset-code")
            .await
            .expect("check action code should succeed");

        mock.assert();
        assert_eq!(info.operation, ActionCodeOperation::PasswordReset);
        assert_eq!(info.data.email.as_deref(), Some(TEST_EMAIL));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn verify_password_reset_code_returns_email() {
        let server = start_mock_server();
        let auth = build_auth(&server);

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:resetPassword")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "oobCode": "reset-code"
                }));
            then.status(200).json_body(json!({
                "email": TEST_EMAIL,
                "requestType": "PASSWORD_RESET"
            }));
        });

        let email = auth
            .verify_password_reset_code("reset-code")
            .await
            .expect("verify password reset code should succeed");

        mock.assert();
        assert_eq!(email, TEST_EMAIL);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn sign_in_with_phone_number_flow() {
        let server = start_mock_server();
        let auth = build_auth(&server);

        let verifier: Arc<dyn ApplicationVerifier> = Arc::new(StaticVerifier {
            token: "recaptcha-token",
            kind: "recaptcha",
        });

        let send_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:sendVerificationCode")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "phoneNumber": "+15551234567",
                    "recaptchaToken": "recaptcha-token",
                    "clientType": CLIENT_TYPE_WEB
                }));
            then.status(200)
                .json_body(json!({ "sessionInfo": "session-info" }));
        });

        let confirmation = auth
            .sign_in_with_phone_number("+15551234567", verifier.clone())
            .await
            .expect("send verification should succeed");
        send_mock.assert();

        let finalize_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:signInWithPhoneNumber")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "sessionInfo": "session-info",
                    "code": "123456"
                }));
            then.status(200).json_body(json!({
                "localId": "phone-uid",
                "idToken": "phone-id-token",
                "refreshToken": "phone-refresh-token",
                "expiresIn": "3600",
                "phoneNumber": "+15551234567",
                "isNewUser": false
            }));
        });

        let lookup_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:lookup")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({ "idToken": "phone-id-token" }));
            then.status(200).json_body(json!({
                "users": [{
                    "localId": "phone-uid",
                    "email": TEST_EMAIL,
                    "emailVerified": true,
                    "phoneNumber": "+15551234567",
                    "mfaInfo": []
                }]
            }));
        });

        let credential = confirmation
            .confirm("123456")
            .await
            .expect("phone confirmation should succeed");
        finalize_mock.assert();
        lookup_mock.assert();

        assert_eq!(credential.user.uid(), "phone-uid");
        assert_eq!(credential.provider_id.as_deref(), Some(PHONE_PROVIDER_ID));
        assert_eq!(credential.operation_type.as_deref(), Some("signIn"));
        assert_eq!(
            credential.user.info().phone_number.as_deref(),
            Some("+15551234567")
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn phone_auth_provider_sign_in_with_credential() {
        let server = start_mock_server();
        let auth = build_auth(&server);

        let verifier: Arc<dyn ApplicationVerifier> = Arc::new(StaticVerifier {
            token: "recaptcha-token",
            kind: "recaptcha",
        });

        let send_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:sendVerificationCode")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "phoneNumber": "+15551234567",
                    "recaptchaToken": "recaptcha-token",
                    "clientType": CLIENT_TYPE_WEB
                }));
            then.status(200)
                .json_body(json!({ "sessionInfo": "provider-session" }));
        });

        let provider = PhoneAuthProvider::new(auth.clone());
        let verification_id = provider
            .verify_phone_number("+15551234567", verifier.clone())
            .await
            .expect("verification should succeed");
        assert_eq!(verification_id, "provider-session");
        send_mock.assert();

        let finalize_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:signInWithPhoneNumber")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "sessionInfo": "provider-session",
                    "code": "123456"
                }));
            then.status(200).json_body(json!({
                "localId": "phone-uid",
                "idToken": "phone-id-token",
                "refreshToken": "phone-refresh-token",
                "expiresIn": "3600",
                "phoneNumber": "+15551234567",
                "isNewUser": false
            }));
        });

        let lookup_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:lookup")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({ "idToken": "phone-id-token" }));
            then.status(200).json_body(json!({
                "users": [{
                    "localId": "phone-uid",
                    "email": TEST_EMAIL,
                    "emailVerified": true,
                    "phoneNumber": "+15551234567",
                    "mfaInfo": []
                }]
            }));
        });

        let credential = PhoneAuthProvider::credential(&verification_id, "123456");
        let result = provider
            .sign_in_with_credential(credential)
            .await
            .expect("sign-in should succeed");

        finalize_mock.assert();
        lookup_mock.assert();

        assert_eq!(result.user.uid(), "phone-uid");
        assert_eq!(result.provider_id.as_deref(), Some(PHONE_PROVIDER_ID));
        assert_eq!(result.operation_type.as_deref(), Some("signIn"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn multi_factor_phone_enrollment_flow() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server).await;

        let verifier: Arc<dyn ApplicationVerifier> = Arc::new(StaticVerifier {
            token: "recaptcha-token",
            kind: "recaptcha",
        });

        let enroll_start = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts/mfaEnrollment:start")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "idToken": TEST_ID_TOKEN,
                    "phoneEnrollmentInfo": {
                        "phoneNumber": "+15551234567",
                        "recaptchaToken": "recaptcha-token",
                        "clientType": CLIENT_TYPE_WEB
                    }
                }));
            then.status(200).json_body(json!({
                "phoneSessionInfo": {"sessionInfo": "mfa-session"}
            }));
        });

        let enroll_finalize = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts/mfaEnrollment:finalize")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "idToken": TEST_ID_TOKEN,
                    "phoneVerificationInfo": {
                        "sessionInfo": "mfa-session",
                        "code": "654321"
                    },
                    "displayName": "Personal phone"
                }));
            then.status(200).json_body(json!({
                "idToken": "mfa-id-token",
                "refreshToken": "mfa-refresh-token"
            }));
        });

        let lookup = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:lookup")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({"idToken": "mfa-id-token"}));
            then.status(200).json_body(json!({
                "users": [{
                    "localId": TEST_UID,
                    "email": TEST_EMAIL,
                    "emailVerified": true,
                    "mfaInfo": [{
                        "mfaEnrollmentId": "enrollment-id",
                        "displayName": "Personal phone",
                        "phoneInfo": "+15551234567",
                        "enrolledAt": "2023-01-01T00:00:00Z"
                    }]
                }]
            }));
        });

        let mfa_user = auth.multi_factor();
        let confirmation = mfa_user
            .enroll_phone_number("+15551234567", verifier, Some("Personal phone"))
            .await
            .expect("start MFA enrollment");
        enroll_start.assert();

        let credential = confirmation
            .confirm("654321")
            .await
            .expect("finalize MFA enrollment");

        enroll_finalize.assert();
        lookup.assert();

        assert_eq!(credential.operation_type.as_deref(), Some("enroll"));
        assert_eq!(credential.provider_id.as_deref(), Some(PHONE_PROVIDER_ID));

        let factors = mfa_user
            .enrolled_factors()
            .await
            .expect("load enrolled factors");
        assert_eq!(factors.len(), 1);
        assert_eq!(factors[0].uid, "enrollment-id");
        assert_eq!(factors[0].factor_id, "phone");

        let session = mfa_user.get_session().await.expect("create session");
        assert_eq!(session.credential(), "mfa-id-token");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn send_email_verification_uses_current_user_token() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server).await;

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
            .await
            .expect("email verification should succeed");

        mock.assert();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn confirm_password_reset_posts_new_password() {
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
            .await
            .expect("confirm reset should succeed");

        mock.assert();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn update_profile_sets_display_name() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server).await;

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
            .await
            .expect("update profile should succeed");

        mock.assert();
        assert_eq!(user.info().display_name.as_deref(), Some("New Name"));
        assert_eq!(
            user.token_manager().access_token(),
            Some(TEST_ID_TOKEN.to_string())
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn update_profile_clears_display_name_when_empty_string() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server).await;

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
            .await
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
            .await
            .expect("clear update should succeed");

        clear_mock.assert();
        assert!(user.info().display_name.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn update_email_sets_new_email() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server).await;

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
            .await
            .expect("update email should succeed");

        mock.assert();
        assert_eq!(user.info().email.as_deref(), Some("new@example.com"));
        assert_eq!(
            user.token_manager().access_token(),
            Some(UPDATED_ID_TOKEN.to_string())
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn update_password_refreshes_tokens() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server).await;

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
            .await
            .expect("update password should succeed");

        mock.assert();
        assert_eq!(user.uid(), TEST_UID);
        assert_eq!(
            user.token_manager().refresh_token(),
            Some(UPDATED_REFRESH_TOKEN.to_string())
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn delete_user_clears_current_user_state() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server).await;

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:delete")
                .query_param("key", TEST_API_KEY)
                .json_body(json!({
                    "idToken": TEST_ID_TOKEN
                }));
            then.status(200);
        });

        auth.delete_user()
            .await
            .expect("delete user should succeed");

        mock.assert();
        assert!(auth.current_user().is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reauthenticate_with_password_updates_current_user() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server).await;

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
            .await
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

    #[tokio::test(flavor = "current_thread")]
    async fn unlink_providers_sends_delete_provider() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server).await;

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
            .await
            .expect("unlink should succeed");

        mock.assert();
        assert_eq!(user.uid(), TEST_UID);
        assert_eq!(user.info().provider_id, EmailAuthProvider::PROVIDER_ID);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn unlink_providers_propagates_errors() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server).await;

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:update")
                .query_param("key", TEST_API_KEY);
            then.status(400)
                .body("{\"error\":{\"message\":\"INVALID_PROVIDER_ID\"}}");
        });

        let result = auth.unlink_providers(&[GOOGLE_PROVIDER_ID]).await;

        mock.assert();
        assert!(matches!(
            result,
            Err(AuthError::InvalidCredential(message)) if message == "INVALID_PROVIDER_ID"
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn get_account_info_returns_users() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server).await;

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
            .await
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

    #[tokio::test(flavor = "current_thread")]
    async fn get_account_info_propagates_errors() {
        let server = start_mock_server();
        let auth = build_auth(&server);
        sign_in_user(&auth, &server).await;

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/accounts:lookup")
                .query_param("key", TEST_API_KEY);
            then.status(400)
                .body("{\"error\":{\"message\":\"INVALID_ID_TOKEN\"}}");
        });

        let result = auth.get_account_info().await;

        mock.assert();
        assert!(matches!(
            result,
            Err(AuthError::InvalidCredential(message)) if message == "INVALID_ID_TOKEN"
        ));
    }
}
