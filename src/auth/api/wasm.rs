use std::sync::Arc;

use crate::app::FirebaseApp;
use crate::auth::error::{AuthError, AuthResult};
use crate::auth::model::{
    AuthCredential, AuthStateListeners, EmailAuthProvider, User, UserCredential,
};
use crate::auth::oauth::{
    OAuthPopupHandler, OAuthRedirectHandler, PendingRedirectEvent, RedirectOperation,
    RedirectPersistence,
};
use crate::auth::persistence::{AuthPersistence, InMemoryPersistence};
use crate::platform::token::{AsyncTokenProvider, TokenError};
use crate::util::PartialObserver;
use async_trait::async_trait;

fn not_supported() -> AuthError {
    AuthError::NotImplemented("auth is not yet supported on wasm32".into())
}

pub struct Auth {
    app: FirebaseApp,
    listeners: AuthStateListeners,
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
        _persistence: Arc<dyn AuthPersistence + Send + Sync>,
    ) -> AuthResult<Self> {
        Ok(Self {
            app,
            listeners: AuthStateListeners::default(),
        })
    }

    pub fn initialize(self: &Arc<Self>) -> AuthResult<()> {
        Ok(())
    }

    pub fn app(&self) -> &FirebaseApp {
        &self.app
    }

    pub fn current_user(&self) -> Option<Arc<User>> {
        None
    }

    pub fn sign_out(&self) {}

    pub fn email_auth_provider(&self) -> EmailAuthProvider {
        EmailAuthProvider
    }

    pub async fn sign_in_with_email_and_password(
        &self,
        _email: &str,
        _password: &str,
    ) -> AuthResult<UserCredential> {
        Err(not_supported())
    }

    pub async fn create_user_with_email_and_password(
        &self,
        _email: &str,
        _password: &str,
    ) -> AuthResult<UserCredential> {
        Err(not_supported())
    }

    pub fn on_auth_state_changed(
        &self,
        observer: PartialObserver<Arc<User>>,
    ) -> impl FnOnce() + Send + 'static {
        self.listeners.add_observer(observer);
        || {}
    }

    pub async fn get_token(&self, _force_refresh: bool) -> AuthResult<Option<String>> {
        Err(not_supported())
    }

    pub async fn get_token_async(&self, _force_refresh: bool) -> AuthResult<Option<String>> {
        self.get_token(_force_refresh).await
    }

    pub fn set_oauth_request_uri(&self, _value: impl Into<String>) {}

    pub fn oauth_request_uri(&self) -> String {
        String::new()
    }

    pub fn set_identity_toolkit_endpoint(&self, _endpoint: impl Into<String>) {}

    pub fn identity_toolkit_endpoint(&self) -> String {
        String::new()
    }

    pub fn set_secure_token_endpoint(&self, _endpoint: impl Into<String>) {}

    pub fn set_popup_handler(&self, _handler: Arc<dyn OAuthPopupHandler>) {}

    pub fn clear_popup_handler(&self) {}

    pub fn popup_handler(&self) -> Option<Arc<dyn OAuthPopupHandler>> {
        None
    }

    pub fn set_redirect_handler(&self, _handler: Arc<dyn OAuthRedirectHandler>) {}

    pub fn clear_redirect_handler(&self) {}

    pub fn redirect_handler(&self) -> Option<Arc<dyn OAuthRedirectHandler>> {
        None
    }

    pub fn set_redirect_persistence(&self, _persistence: Arc<dyn RedirectPersistence>) {}

    pub(crate) fn set_pending_redirect_event(
        &self,
        _provider_id: &str,
        _operation: RedirectOperation,
    ) -> AuthResult<()> {
        Err(not_supported())
    }

    pub(crate) fn clear_pending_redirect_event(&self) -> AuthResult<()> {
        Err(not_supported())
    }

    pub(crate) fn take_pending_redirect_event(&self) -> AuthResult<Option<PendingRedirectEvent>> {
        Err(not_supported())
    }

    pub async fn sign_in_with_oauth_credential(
        &self,
        _credential: AuthCredential,
    ) -> AuthResult<UserCredential> {
        Err(not_supported())
    }

    pub async fn send_password_reset_email(&self, _email: &str) -> AuthResult<()> {
        Err(not_supported())
    }

    pub async fn confirm_password_reset(
        &self,
        _oob_code: &str,
        _new_password: &str,
    ) -> AuthResult<()> {
        Err(not_supported())
    }

    pub async fn send_email_verification(&self) -> AuthResult<()> {
        Err(not_supported())
    }

    pub async fn update_profile(
        &self,
        _display_name: Option<&str>,
        _photo_url: Option<&str>,
    ) -> AuthResult<Arc<User>> {
        Err(not_supported())
    }

    pub async fn update_email(&self, _email: &str) -> AuthResult<Arc<User>> {
        Err(not_supported())
    }

    pub async fn update_password(&self, _password: &str) -> AuthResult<Arc<User>> {
        Err(not_supported())
    }

    pub async fn delete_user(&self) -> AuthResult<()> {
        Err(not_supported())
    }

    pub async fn unlink_providers(&self, _provider_ids: &[&str]) -> AuthResult<Arc<User>> {
        Err(not_supported())
    }

    pub async fn get_account_info(&self) -> AuthResult<crate::auth::model::GetAccountInfoResponse> {
        Err(not_supported())
    }

    pub async fn link_with_oauth_credential(
        &self,
        _credential: AuthCredential,
    ) -> AuthResult<UserCredential> {
        Err(not_supported())
    }

    pub async fn reauthenticate_with_password(
        &self,
        _email: &str,
        _password: &str,
    ) -> AuthResult<Arc<User>> {
        Err(not_supported())
    }

    pub async fn reauthenticate_with_oauth_credential(
        &self,
        _credential: AuthCredential,
    ) -> AuthResult<Arc<User>> {
        Err(not_supported())
    }
}

#[async_trait]
impl AsyncTokenProvider for Arc<Auth> {
    async fn get_token(&self, force_refresh: bool) -> Result<Option<String>, TokenError> {
        self.get_token(force_refresh)
            .await
            .map_err(TokenError::from_error)
    }
}

pub struct AuthBuilder {
    app: FirebaseApp,
}

impl AuthBuilder {
    fn new(app: FirebaseApp) -> Self {
        Self { app }
    }

    pub fn with_persistence(self, _persistence: Arc<dyn AuthPersistence + Send + Sync>) -> Self {
        self
    }

    pub fn with_popup_handler(self, _handler: Arc<dyn OAuthPopupHandler>) -> Self {
        self
    }

    pub fn with_redirect_handler(self, _handler: Arc<dyn OAuthRedirectHandler>) -> Self {
        self
    }

    pub fn with_oauth_request_uri(self, _request_uri: impl Into<String>) -> Self {
        self
    }

    pub fn with_redirect_persistence(self, _persistence: Arc<dyn RedirectPersistence>) -> Self {
        self
    }

    pub fn with_identity_toolkit_endpoint(self, _endpoint: impl Into<String>) -> Self {
        self
    }

    pub fn with_secure_token_endpoint(self, _endpoint: impl Into<String>) -> Self {
        self
    }

    pub fn defer_initialization(self) -> Self {
        self
    }

    pub fn build(self) -> AuthResult<Arc<Auth>> {
        let auth = Auth::new(self.app)?;
        Ok(Arc::new(auth))
    }
}

pub fn register_auth_component() {}

pub fn auth_for_app(_app: FirebaseApp) -> AuthResult<Arc<Auth>> {
    Err(not_supported())
}
