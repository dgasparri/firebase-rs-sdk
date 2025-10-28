use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::app::FirebaseApp;
use crate::auth::api::Auth;
use crate::auth::error::{AuthError, AuthResult};
use crate::auth::model::{User, UserCredential};
use crate::util::PartialObserver;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdTokenResult {
    pub token: String,
    pub auth_time: Option<String>,
    pub issued_at_time: Option<String>,
    pub expiration_time: Option<String>,
    pub sign_in_provider: Option<String>,
    pub sign_in_second_factor: Option<String>,
    pub claims: Value,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserMetadata {
    pub creation_time: Option<String>,
    pub last_sign_in_time: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionCodeSettings {
    pub url: String,
    pub handle_code_in_app: bool,
    pub i_os: Option<IosSettings>,
    pub android: Option<AndroidSettings>,
    pub dynamic_link_domain: Option<String>,
    pub link_domain: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IosSettings {
    pub bundle_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AndroidSettings {
    pub package_name: String,
    pub install_app: Option<bool>,
    pub minimum_version: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ActionCodeOperation {
    PasswordReset,
    RecoverEmail,
    EmailSignIn,
    RevertSecondFactorAddition,
    VerifyAndChangeEmail,
    #[default]
    VerifyEmail,
}

impl ActionCodeOperation {
    /// Returns the requestType string expected by the Identity Toolkit API.
    pub fn as_request_type(&self) -> &'static str {
        match self {
            ActionCodeOperation::PasswordReset => "PASSWORD_RESET",
            ActionCodeOperation::RecoverEmail => "RECOVER_EMAIL",
            ActionCodeOperation::EmailSignIn => "EMAIL_SIGNIN",
            ActionCodeOperation::RevertSecondFactorAddition => "REVERT_SECOND_FACTOR_ADDITION",
            ActionCodeOperation::VerifyAndChangeEmail => "VERIFY_AND_CHANGE_EMAIL",
            ActionCodeOperation::VerifyEmail => "VERIFY_EMAIL",
        }
    }

    /// Parses a `requestType` string returned by the REST API.
    pub fn from_request_type(value: &str) -> Option<Self> {
        match value {
            "PASSWORD_RESET" => Some(ActionCodeOperation::PasswordReset),
            "RECOVER_EMAIL" => Some(ActionCodeOperation::RecoverEmail),
            "EMAIL_SIGNIN" => Some(ActionCodeOperation::EmailSignIn),
            "REVERT_SECOND_FACTOR_ADDITION" => {
                Some(ActionCodeOperation::RevertSecondFactorAddition)
            }
            "VERIFY_AND_CHANGE_EMAIL" => Some(ActionCodeOperation::VerifyAndChangeEmail),
            "VERIFY_EMAIL" => Some(ActionCodeOperation::VerifyEmail),
            _ => None,
        }
    }

    /// Parses the `mode` query parameter from action code links.
    pub fn from_mode(value: &str) -> Option<Self> {
        match value {
            "recoverEmail" => Some(ActionCodeOperation::RecoverEmail),
            "resetPassword" => Some(ActionCodeOperation::PasswordReset),
            "signIn" => Some(ActionCodeOperation::EmailSignIn),
            "verifyEmail" => Some(ActionCodeOperation::VerifyEmail),
            "verifyAndChangeEmail" => Some(ActionCodeOperation::VerifyAndChangeEmail),
            "revertSecondFactorAddition" => Some(ActionCodeOperation::RevertSecondFactorAddition),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionCodeInfoData {
    pub email: Option<String>,
    pub previous_email: Option<String>,
    pub multi_factor_info: Option<MultiFactorInfo>,
    pub from_email: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionCodeInfo {
    pub data: ActionCodeInfoData,
    pub operation: ActionCodeOperation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionCodeUrl {
    pub api_key: String,
    pub code: String,
    pub continue_url: Option<String>,
    pub language_code: Option<String>,
    pub tenant_id: Option<String>,
    pub operation: ActionCodeOperation,
}

impl ActionCodeUrl {
    /// Parses an out-of-band action link into its structured representation.
    pub fn parse(link: &str) -> Option<Self> {
        let resolved_link = resolve_action_link(link)?;
        let parsed = Url::parse(&resolved_link).ok()?;
        let query: HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        let api_key = query.get("apiKey")?.clone();
        let code = query.get("oobCode")?.clone();
        let operation = query
            .get("mode")
            .and_then(|mode| ActionCodeOperation::from_mode(mode))?;
        let language_code = query
            .get("lang")
            .cloned()
            .or_else(|| query.get("languageCode").cloned());
        Some(Self {
            api_key,
            code,
            continue_url: query.get("continueUrl").cloned(),
            language_code,
            tenant_id: query.get("tenantId").cloned(),
            operation,
        })
    }
}

fn resolve_action_link(link: &str) -> Option<String> {
    fn helper(original: &str, depth: usize) -> Option<String> {
        if depth > 4 {
            return Some(original.to_string());
        }
        let parsed = Url::parse(original).ok()?;
        let query: HashMap<_, _> = parsed.query_pairs().into_owned().collect();

        if let Some(link_value) = query.get("link") {
            if let Some(resolved) = helper(link_value, depth + 1) {
                return Some(resolved);
            }
            return Some(link_value.clone());
        }

        if let Some(deep_link) = query.get("deep_link_id") {
            if let Some(resolved) = helper(deep_link, depth + 1) {
                return Some(resolved);
            }
            return Some(deep_link.clone());
        }

        Some(original.to_string())
    }

    helper(link, 0)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AdditionalUserInfo {
    pub is_new_user: bool,
    pub provider_id: Option<String>,
    pub profile: Option<Value>,
    pub username: Option<String>,
}

#[cfg(target_arch = "wasm32")]
type ConfirmationFuture = Pin<Box<dyn Future<Output = AuthResult<UserCredential>> + 'static>>;

#[cfg(not(target_arch = "wasm32"))]
type ConfirmationFuture =
    Pin<Box<dyn Future<Output = AuthResult<UserCredential>> + Send + 'static>>;

#[cfg(target_arch = "wasm32")]
type ConfirmationHandler = Arc<dyn Fn(&str) -> ConfirmationFuture + 'static>;

#[cfg(not(target_arch = "wasm32"))]
type ConfirmationHandler = Arc<dyn Fn(&str) -> ConfirmationFuture + Send + Sync + 'static>;

pub struct ConfirmationResult {
    verification_id: String,
    confirm_handler: ConfirmationHandler,
}

impl ConfirmationResult {
    /// Creates a confirmation result that can complete sign-in with the provided handler.
    #[cfg(target_arch = "wasm32")]
    pub fn new<F, Fut>(verification_id: String, confirm_handler: F) -> Self
    where
        F: Fn(&str) -> Fut + 'static,
        Fut: Future<Output = AuthResult<UserCredential>> + 'static,
    {
        let handler = move |code: &str| -> ConfirmationFuture {
            let fut = confirm_handler(code);
            Box::pin(fut)
        };
        Self {
            verification_id,
            confirm_handler: Arc::new(handler),
        }
    }

    /// Creates a confirmation result that can complete sign-in with the provided handler.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new<F, Fut>(verification_id: String, confirm_handler: F) -> Self
    where
        F: Fn(&str) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = AuthResult<UserCredential>> + Send + 'static,
    {
        let handler = move |code: &str| -> ConfirmationFuture {
            let fut = confirm_handler(code);
            Box::pin(fut)
        };
        Self {
            verification_id,
            confirm_handler: Arc::new(handler),
        }
    }

    /// Finalizes authentication by providing the SMS verification code.
    pub async fn confirm(&self, verification_code: &str) -> AuthResult<UserCredential> {
        (self.confirm_handler)(verification_code).await
    }

    /// Returns the verification ID that should be paired with the SMS code.
    pub fn verification_id(&self) -> &str {
        &self.verification_id
    }
}

impl Clone for ConfirmationResult {
    fn clone(&self) -> Self {
        Self {
            verification_id: self.verification_id.clone(),
            confirm_handler: self.confirm_handler.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AuthSettings {
    pub app_verification_disabled_for_testing: bool,
}

pub trait ApplicationVerifier: Send + Sync {
    fn verify(&self) -> AuthResult<String>;
    fn verifier_type(&self) -> &str;
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiFactorInfo {
    pub uid: String,
    pub display_name: Option<String>,
    pub enrollment_time: Option<String>,
    pub factor_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiFactorSession;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiFactorAssertion;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiFactorResolver;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiFactorUser;

impl MultiFactorUser {
    /// Returns the list of enrolled multi-factor authenticators.
    pub fn enrolled_factors(&self) -> Vec<MultiFactorInfo> {
        Vec::new()
    }

    /// Attempts to enroll a new multi-factor assertion (not yet implemented).
    pub fn enroll(
        &self,
        _assertion: MultiFactorAssertion,
        _display_name: Option<&str>,
    ) -> AuthResult<()> {
        Err(AuthError::NotImplemented("multi-factor enrollment"))
    }

    /// Requests a multi-factor session for subsequent operations.
    pub fn get_session(&self) -> AuthResult<MultiFactorSession> {
        Err(AuthError::NotImplemented("multi-factor session"))
    }

    /// Removes an enrolled multi-factor authenticator.
    pub fn unenroll(&self, _factor: &MultiFactorInfo) -> AuthResult<()> {
        Err(AuthError::NotImplemented("multi-factor unenroll"))
    }
}

#[derive(Clone)]
pub struct AuthStateListener {
    pub observer: PartialObserver<Arc<User>>,
}

impl AuthStateListener {
    /// Wraps an observer so it can be registered with the Auth state machine.
    pub fn new(observer: PartialObserver<Arc<User>>) -> Self {
        Self { observer }
    }
}

pub type Observer<T> = PartialObserver<T>;

#[derive(Clone)]
pub struct FirebaseAuth {
    inner: Arc<Auth>,
}

impl FirebaseAuth {
    /// Creates a high-level Auth façade around the shared `Auth` core.
    pub fn new(inner: Arc<Auth>) -> Self {
        Self { inner }
    }

    /// Returns the `FirebaseApp` associated with this Auth instance.
    pub fn app(&self) -> &FirebaseApp {
        self.inner.app()
    }

    /// Returns the currently signed-in user, if any.
    pub fn current_user(&self) -> Option<Arc<User>> {
        self.inner.current_user()
    }

    /// Signs the current user out of Firebase Auth.
    pub fn sign_out(&self) {
        self.inner.sign_out();
    }

    /// Signs a user in with an email and password.
    pub async fn sign_in_with_email_and_password(
        &self,
        email: &str,
        password: &str,
    ) -> AuthResult<UserCredential> {
        self.inner
            .sign_in_with_email_and_password(email, password)
            .await
    }

    /// Creates a new user with the provided email and password.
    pub async fn create_user_with_email_and_password(
        &self,
        email: &str,
        password: &str,
    ) -> AuthResult<UserCredential> {
        self.inner
            .create_user_with_email_and_password(email, password)
            .await
    }

    /// Registers an observer that is notified whenever the auth state changes.
    pub fn on_auth_state_changed(
        &self,
        observer: PartialObserver<Arc<User>>,
    ) -> impl FnOnce() + Send + 'static {
        self.inner.on_auth_state_changed(observer)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::error::AuthError;

    #[tokio::test(flavor = "current_thread")]
    async fn confirmation_result_invokes_handler() {
        let result = ConfirmationResult::new("verification_id".into(), |code| {
            let code = code.to_string();
            async move {
                assert_eq!(code, "123456");
                Err(AuthError::NotImplemented("test"))
            }
        });
        assert!(result.confirm("123456").await.is_err());
    }
}
