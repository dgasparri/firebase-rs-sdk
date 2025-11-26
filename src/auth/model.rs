use crate::app::FirebaseApp;
use crate::auth::error::{AuthError, AuthResult};
use crate::auth::token_manager::{TokenManager, TokenUpdate};
use crate::auth::types::MultiFactorInfo;
use crate::util::PartialObserver;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInfo {
    pub uid: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub phone_number: Option<String>,
    pub photo_url: Option<String>,
    pub provider_id: String,
}

#[derive(Clone, Debug)]
pub struct User {
    app: FirebaseApp,
    info: UserInfo,
    email_verified: bool,
    is_anonymous: bool,
    token_manager: TokenManager,
    mfa_factors: Arc<Mutex<Vec<MultiFactorInfo>>>,
}

impl User {
    /// Creates a new user bound to the given app with profile information.
    pub fn new(app: FirebaseApp, info: UserInfo) -> Self {
        Self {
            app,
            info,
            email_verified: false,
            is_anonymous: false,
            token_manager: TokenManager::default(),
            mfa_factors: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Returns the owning `FirebaseApp` for the user.
    pub fn app(&self) -> &FirebaseApp {
        &self.app
    }

    /// Indicates whether the user signed in anonymously.
    pub fn is_anonymous(&self) -> bool {
        self.is_anonymous
    }

    /// Flags the user as anonymous or regular.
    pub fn set_anonymous(&mut self, anonymous: bool) {
        self.is_anonymous = anonymous;
    }

    /// Returns the stable Firebase UID for the user.
    pub fn uid(&self) -> &str {
        &self.info.uid
    }

    /// Indicates whether the user's email has been verified.
    pub fn email_verified(&self) -> bool {
        self.email_verified
    }

    /// Returns the refresh token issued for this user, if present.
    pub fn refresh_token(&self) -> Option<String> {
        self.token_manager.refresh_token()
    }

    /// Returns the cached ID token or an error if none is available.
    pub fn get_id_token(&self, _force_refresh: bool) -> AuthResult<String> {
        self.token_manager
            .access_token()
            .ok_or_else(|| AuthError::InvalidCredential("Missing ID token".into()))
    }

    /// Exposes the underlying token manager.
    pub fn token_manager(&self) -> &TokenManager {
        &self.token_manager
    }

    /// Updates the cached tokens with fresh credentials from the backend.
    pub fn update_tokens(
        &self,
        access_token: Option<String>,
        refresh_token: Option<String>,
        expires_in: Option<Duration>,
    ) {
        let update = TokenUpdate::new(access_token, refresh_token, expires_in);
        self.token_manager.update(update);
    }

    /// Returns the immutable `UserInfo` profile snapshot.
    pub fn info(&self) -> &UserInfo {
        &self.info
    }

    pub(crate) fn set_email_verified(&mut self, value: bool) {
        self.email_verified = value;
    }

    pub(crate) fn set_mfa_info(&self, factors: Vec<MultiFactorInfo>) {
        *self.mfa_factors.lock().unwrap() = factors;
    }

    pub fn mfa_info(&self) -> Vec<MultiFactorInfo> {
        self.mfa_factors.lock().unwrap().clone()
    }
}

#[derive(Clone, Debug)]
pub struct UserCredential {
    pub user: Arc<User>,
    pub provider_id: Option<String>,
    pub operation_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AuthCredential {
    pub provider_id: String,
    pub sign_in_method: String,
    pub token_response: serde_json::Value,
}

impl AuthCredential {
    /// Serializes the credential into a JSON value matching the Firebase JS SDK shape.
    pub fn to_json(&self) -> Value {
        json!({
            "providerId": self.provider_id,
            "signInMethod": self.sign_in_method,
            "tokenResponse": self.token_response,
        })
    }

    /// Serializes the credential into a JSON string.
    pub fn to_json_string(&self) -> AuthResult<String> {
        serde_json::to_string(&self.to_json()).map_err(|err| AuthError::InvalidCredential(err.to_string()))
    }

    /// Reconstructs a credential from a JSON value previously produced via [`to_json`].
    pub fn from_json(value: Value) -> AuthResult<Self> {
        let provider_id = value
            .get("providerId")
            .and_then(Value::as_str)
            .ok_or_else(|| AuthError::InvalidCredential("Credential JSON missing providerId".into()))?
            .to_string();

        let sign_in_method = value
            .get("signInMethod")
            .and_then(Value::as_str)
            .ok_or_else(|| AuthError::InvalidCredential("Credential JSON missing signInMethod".into()))?
            .to_string();

        let token_response = value.get("tokenResponse").cloned().unwrap_or_else(|| json!({}));

        Ok(Self {
            provider_id,
            sign_in_method,
            token_response,
        })
    }

    /// Reconstructs a credential from its JSON string representation.
    pub fn from_json_str(data: &str) -> AuthResult<Self> {
        let value: Value = serde_json::from_str(data).map_err(|err| AuthError::InvalidCredential(err.to_string()))?;
        Self::from_json(value)
    }
}

#[derive(Debug, Clone, Default)]
pub struct AuthConfig {
    pub api_key: Option<String>,
    pub identity_toolkit_endpoint: Option<String>,
    pub secure_token_endpoint: Option<String>,
}

#[derive(Clone)]
pub struct EmailAuthProvider;

impl EmailAuthProvider {
    pub const PROVIDER_ID: &'static str = "password";

    /// Builds an auth credential suitable for email/password sign-in flows.
    pub fn credential(email: &str, password: &str) -> AuthCredential {
        AuthCredential {
            provider_id: Self::PROVIDER_ID.to_string(),
            sign_in_method: Self::PROVIDER_ID.to_string(),
            token_response: json!({
                "email": email,
                "password": password,
                "returnSecureToken": true,
            }),
        }
    }
}

#[derive(Default)]
pub struct AuthStateListeners {
    observers: Mutex<Vec<PartialObserver<Arc<User>>>>,
}

impl AuthStateListeners {
    /// Registers a new observer to receive auth state changes.
    pub fn add_observer(&self, observer: PartialObserver<Arc<User>>) {
        self.observers.lock().unwrap().push(observer);
    }

    /// Notifies all observers with the provided user snapshot.
    pub fn notify(&self, user: Arc<User>) {
        for observer in self.observers.lock().unwrap().iter() {
            if let Some(next) = observer.next.clone() {
                next(&user);
            }
        }
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct SignInWithPasswordRequest {
    pub email: String,
    pub password: String,
    #[serde(rename = "returnSecureToken")]
    pub return_secure_token: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SignInWithPasswordResponse {
    #[serde(rename = "idToken")]
    pub id_token: Option<String>,
    #[serde(rename = "refreshToken")]
    pub refresh_token: Option<String>,
    #[serde(rename = "localId")]
    pub local_id: String,
    pub email: String,
    #[serde(rename = "expiresIn")]
    pub expires_in: Option<String>,
    #[serde(rename = "mfaPendingCredential")]
    pub mfa_pending_credential: Option<String>,
    #[serde(rename = "mfaInfo")]
    pub mfa_info: Option<Vec<MfaEnrollmentInfo>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_credential_json_roundtrip() {
        let credential = AuthCredential {
            provider_id: "custom-provider".into(),
            sign_in_method: "custom-provider".into(),
            token_response: json!({ "idToken": "abc" }),
        };

        let json_value = credential.to_json();
        let restored = AuthCredential::from_json(json_value.clone()).unwrap();
        assert_eq!(restored.provider_id, "custom-provider");
        assert_eq!(restored.sign_in_method, "custom-provider");
        assert_eq!(restored.token_response, json_value["tokenResponse"]);

        let json_string = credential.to_json_string().unwrap();
        let restored_from_str = AuthCredential::from_json_str(&json_string).unwrap();
        assert_eq!(restored_from_str.provider_id, "custom-provider");
        assert_eq!(restored_from_str.sign_in_method, "custom-provider");
    }
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct SignUpRequest {
    #[serde(rename = "idToken", skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    #[serde(rename = "returnSecureToken", skip_serializing_if = "Option::is_none")]
    pub return_secure_token: Option<bool>,
    #[serde(rename = "email", skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(rename = "password", skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(rename = "captchaResponse", skip_serializing_if = "Option::is_none")]
    pub captcha_response: Option<String>,
    #[serde(rename = "clientType", skip_serializing_if = "Option::is_none")]
    pub client_type: Option<String>,
    #[serde(rename = "recaptchaVersion", skip_serializing_if = "Option::is_none")]
    pub recaptcha_version: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SignUpResponse {
    #[serde(rename = "idToken")]
    pub id_token: Option<String>,
    #[serde(rename = "refreshToken")]
    pub refresh_token: Option<String>,
    #[serde(rename = "localId")]
    pub local_id: Option<String>,
    #[serde(rename = "email")]
    pub email: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(rename = "expiresIn")]
    pub expires_in: Option<String>,
    #[serde(rename = "isNewUser")]
    pub is_new_user: Option<bool>,
    #[serde(rename = "mfaPendingCredential")]
    pub mfa_pending_credential: Option<String>,
    #[serde(rename = "mfaInfo")]
    pub mfa_info: Option<Vec<MfaEnrollmentInfo>>,
}

#[derive(Debug, Serialize, Clone)]
pub struct SignInWithCustomTokenRequest {
    #[serde(rename = "token")]
    pub token: String,
    #[serde(rename = "returnSecureToken")]
    pub return_secure_token: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SignInWithCustomTokenResponse {
    #[serde(rename = "idToken")]
    pub id_token: Option<String>,
    #[serde(rename = "refreshToken")]
    pub refresh_token: Option<String>,
    #[serde(rename = "localId")]
    pub local_id: Option<String>,
    #[serde(rename = "email")]
    pub email: Option<String>,
    #[serde(rename = "expiresIn")]
    pub expires_in: Option<String>,
    #[serde(rename = "isNewUser")]
    pub is_new_user: Option<bool>,
    #[serde(rename = "mfaPendingCredential")]
    pub mfa_pending_credential: Option<String>,
    #[serde(rename = "mfaInfo")]
    pub mfa_info: Option<Vec<MfaEnrollmentInfo>>,
}

#[derive(Debug, Serialize, Clone)]
pub struct SignInWithEmailLinkRequest {
    #[serde(rename = "email")]
    pub email: String,
    #[serde(rename = "oobCode")]
    pub oob_code: String,
    #[serde(rename = "returnSecureToken")]
    pub return_secure_token: bool,
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(rename = "idToken", skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SignInWithEmailLinkResponse {
    #[serde(rename = "idToken")]
    pub id_token: Option<String>,
    #[serde(rename = "refreshToken")]
    pub refresh_token: Option<String>,
    #[serde(rename = "localId")]
    pub local_id: Option<String>,
    #[serde(rename = "email")]
    pub email: Option<String>,
    #[serde(rename = "expiresIn")]
    pub expires_in: Option<String>,
    #[serde(rename = "isNewUser")]
    pub is_new_user: Option<bool>,
    #[serde(rename = "mfaPendingCredential")]
    pub mfa_pending_credential: Option<String>,
    #[serde(rename = "mfaInfo")]
    pub mfa_info: Option<Vec<MfaEnrollmentInfo>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderUserInfo {
    #[serde(rename = "providerId")]
    pub provider_id: Option<String>,
    #[serde(rename = "rawId")]
    pub raw_id: Option<String>,
    #[serde(rename = "email")]
    pub email: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(rename = "photoUrl")]
    pub photo_url: Option<String>,
    #[serde(rename = "phoneNumber")]
    pub phone_number: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MfaEnrollmentInfo {
    #[serde(rename = "mfaEnrollmentId")]
    pub mfa_enrollment_id: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(rename = "phoneInfo")]
    pub phone_info: Option<String>,
    #[serde(rename = "totpInfo")]
    pub totp_info: Option<Value>,
    #[serde(rename = "webauthnInfo")]
    pub webauthn_info: Option<Value>,
    #[serde(rename = "enrolledAt")]
    pub enrolled_at: Option<Value>,
    #[serde(rename = "factorId")]
    pub factor_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccountInfoUser {
    #[serde(rename = "localId")]
    pub local_id: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(rename = "photoUrl")]
    pub photo_url: Option<String>,
    #[serde(rename = "email")]
    pub email: Option<String>,
    #[serde(rename = "emailVerified")]
    pub email_verified: Option<bool>,
    #[serde(rename = "phoneNumber")]
    pub phone_number: Option<String>,
    #[serde(rename = "providerUserInfo")]
    pub provider_user_info: Option<Vec<ProviderUserInfo>>,
    #[serde(rename = "mfaInfo")]
    pub mfa_info: Option<Vec<MfaEnrollmentInfo>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetAccountInfoResponse {
    pub users: Vec<AccountInfoUser>,
}
