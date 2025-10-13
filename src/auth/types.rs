use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionCodeOperation {
    PasswordReset,
    RecoverEmail,
    EmailSignIn,
    RevertSecondFactorAddition,
    VerifyAndChangeEmail,
    VerifyEmail,
}

impl Default for ActionCodeOperation {
    fn default() -> Self {
        ActionCodeOperation::VerifyEmail
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
    pub fn parse(link: &str) -> Option<Self> {
        let parsed = Url::parse(link).ok()?;
        let query: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        let api_key = query.get("apiKey")?.clone();
        let code = query.get("oobCode")?.clone();
        Some(Self {
            api_key,
            code,
            continue_url: query.get("continueUrl").cloned(),
            language_code: query.get("languageCode").cloned(),
            tenant_id: query.get("tenantId").cloned(),
            operation: ActionCodeOperation::EmailSignIn,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdditionalUserInfo {
    pub is_new_user: bool,
    pub provider_id: Option<String>,
    pub profile: Option<Value>,
    pub username: Option<String>,
}

impl Default for AdditionalUserInfo {
    fn default() -> Self {
        Self {
            is_new_user: false,
            provider_id: None,
            profile: None,
            username: None,
        }
    }
}

#[derive(Clone)]
pub struct ConfirmationResult {
    verification_id: String,
    confirm_handler: Arc<dyn Fn(&str) -> AuthResult<UserCredential> + Send + Sync + 'static>,
}

impl ConfirmationResult {
    pub fn new<F>(verification_id: String, confirm_handler: F) -> Self
    where
        F: Fn(&str) -> AuthResult<UserCredential> + Send + Sync + 'static,
    {
        Self {
            verification_id,
            confirm_handler: Arc::new(confirm_handler),
        }
    }

    pub fn confirm(&self, verification_code: &str) -> AuthResult<UserCredential> {
        (self.confirm_handler)(verification_code)
    }

    pub fn verification_id(&self) -> &str {
        &self.verification_id
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthSettings {
    pub app_verification_disabled_for_testing: bool,
}

impl Default for AuthSettings {
    fn default() -> Self {
        Self {
            app_verification_disabled_for_testing: false,
        }
    }
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
    pub fn enrolled_factors(&self) -> Vec<MultiFactorInfo> {
        Vec::new()
    }

    pub fn enroll(
        &self,
        _assertion: MultiFactorAssertion,
        _display_name: Option<&str>,
    ) -> AuthResult<()> {
        Err(AuthError::NotImplemented("multi-factor enrollment"))
    }

    pub fn get_session(&self) -> AuthResult<MultiFactorSession> {
        Err(AuthError::NotImplemented("multi-factor session"))
    }

    pub fn unenroll(&self, _factor: &MultiFactorInfo) -> AuthResult<()> {
        Err(AuthError::NotImplemented("multi-factor unenroll"))
    }
}

#[derive(Clone)]
pub struct AuthStateListener {
    pub observer: PartialObserver<Arc<User>>,
}

impl AuthStateListener {
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
    pub fn new(inner: Arc<Auth>) -> Self {
        Self { inner }
    }

    pub fn app(&self) -> &FirebaseApp {
        self.inner.app()
    }

    pub fn current_user(&self) -> Option<Arc<User>> {
        self.inner.current_user()
    }

    pub fn sign_out(&self) {
        self.inner.sign_out();
    }

    pub fn sign_in_with_email_and_password(
        &self,
        email: &str,
        password: &str,
    ) -> AuthResult<UserCredential> {
        self.inner.sign_in_with_email_and_password(email, password)
    }

    pub fn create_user_with_email_and_password(
        &self,
        email: &str,
        password: &str,
    ) -> AuthResult<UserCredential> {
        self.inner
            .create_user_with_email_and_password(email, password)
    }

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

    #[test]
    fn confirmation_result_invokes_handler() {
        let result = ConfirmationResult::new("verification_id".into(), |code| {
            assert_eq!(code, "123456");
            Err(AuthError::NotImplemented("test"))
        });
        assert!(result.confirm("123456").is_err());
    }
}
