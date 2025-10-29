use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Weak};
use std::time::SystemTime;
use url::form_urlencoded::byte_serialize;

use crate::app::FirebaseApp;
use crate::auth::api::Auth;
use crate::auth::error::{AuthError, AuthResult};
use crate::auth::model::{MfaEnrollmentInfo, User, UserCredential};
use crate::auth::phone::PhoneAuthCredential;
use crate::auth::PHONE_PROVIDER_ID;
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

impl MultiFactorInfo {
    pub(crate) fn from_enrollment(enrollment: &MfaEnrollmentInfo) -> Option<Self> {
        let uid = enrollment.mfa_enrollment_id.clone()?;
        let factor_id = enrollment
            .factor_id
            .clone()
            .or_else(|| {
                enrollment
                    .phone_info
                    .as_ref()
                    .map(|_| PHONE_PROVIDER_ID.to_string())
            })
            .unwrap_or_else(|| "unknown".to_string());

        let display_name = if factor_id == WEBAUTHN_FACTOR_ID {
            enrollment.display_name.clone().or_else(|| {
                enrollment
                    .webauthn_info
                    .as_ref()
                    .and_then(|info| info.get("displayName"))
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string())
            })
        } else {
            enrollment.display_name.clone()
        };

        Some(Self {
            uid,
            display_name,
            enrollment_time: enrollment
                .enrolled_at
                .as_ref()
                .map(|value| value.to_string()),
            factor_id,
        })
    }
}

/// Distinguishes between enrollment and sign-in multi-factor sessions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MultiFactorSessionType {
    /// A session captured during enrollment flows (contains an ID token).
    Enrollment,
    /// A session captured during sign-in flows (contains an MFA pending credential).
    SignIn,
}

/// Indicates which primary flow triggered a multi-factor requirement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MultiFactorOperation {
    /// Multi-factor is required while completing a sign-in.
    SignIn,
    /// Multi-factor is required while completing a reauthentication.
    Reauthenticate,
    /// Multi-factor is required while completing a link operation.
    Link,
}

#[derive(Clone, Debug)]
pub struct MultiFactorSession {
    kind: MultiFactorSessionType,
    credential: String,
}

impl MultiFactorSession {
    pub(crate) fn enrollment(id_token: String) -> Self {
        Self {
            kind: MultiFactorSessionType::Enrollment,
            credential: id_token,
        }
    }

    pub(crate) fn sign_in(pending_credential: String) -> Self {
        Self {
            kind: MultiFactorSessionType::SignIn,
            credential: pending_credential,
        }
    }

    /// Returns the raw credential captured for this session.
    ///
    /// For enrollment sessions this is the user's ID token, while for sign-in sessions it represents
    /// the `mfaPendingCredential` returned by the server.
    pub fn credential(&self) -> &str {
        &self.credential
    }

    /// Returns the type of multi-factor session that was established.
    pub fn session_type(&self) -> MultiFactorSessionType {
        self.kind
    }

    /// Returns the ID token captured for enrollment sessions.
    pub fn id_token(&self) -> Option<&str> {
        match self.kind {
            MultiFactorSessionType::Enrollment => Some(&self.credential),
            MultiFactorSessionType::SignIn => None,
        }
    }

    /// Returns the pending credential captured for sign-in sessions.
    pub fn pending_credential(&self) -> Option<&str> {
        match self.kind {
            MultiFactorSessionType::SignIn => Some(&self.credential),
            MultiFactorSessionType::Enrollment => None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MultiFactorSignInContext {
    pub local_id: Option<String>,
    pub email: Option<String>,
    pub phone_number: Option<String>,
    pub provider_id: Option<String>,
    pub is_new_user: Option<bool>,
    pub anonymous: bool,
}

impl Default for MultiFactorSignInContext {
    fn default() -> Self {
        Self {
            local_id: None,
            email: None,
            phone_number: None,
            provider_id: None,
            is_new_user: None,
            anonymous: false,
        }
    }
}

impl MultiFactorSignInContext {
    pub(crate) fn operation_label(&self, operation: MultiFactorOperation) -> &'static str {
        match operation {
            MultiFactorOperation::SignIn => {
                if self.is_new_user.unwrap_or(false) {
                    "signUp"
                } else {
                    "signIn"
                }
            }
            MultiFactorOperation::Reauthenticate => "reauthenticate",
            MultiFactorOperation::Link => "link",
        }
    }
}

#[derive(Clone, Debug)]
pub struct PhoneMultiFactorAssertion {
    credential: PhoneAuthCredential,
}

impl PhoneMultiFactorAssertion {
    pub(crate) fn new(credential: PhoneAuthCredential) -> Self {
        Self { credential }
    }

    pub(crate) fn credential(&self) -> &PhoneAuthCredential {
        &self.credential
    }
}

#[derive(Clone, Debug)]
pub struct TotpSecret {
    secret_key: String,
    hashing_algorithm: String,
    code_length: u32,
    code_interval_seconds: u32,
    enrollment_deadline: SystemTime,
    session_info: String,
    auth: Weak<Auth>,
}

impl TotpSecret {
    pub(crate) fn new(
        auth: &Arc<Auth>,
        secret_key: String,
        hashing_algorithm: String,
        code_length: u32,
        code_interval_seconds: u32,
        enrollment_deadline: SystemTime,
        session_info: String,
    ) -> Self {
        Self {
            secret_key,
            hashing_algorithm,
            code_length,
            code_interval_seconds,
            enrollment_deadline,
            session_info,
            auth: Arc::downgrade(auth),
        }
    }

    pub fn secret_key(&self) -> &str {
        &self.secret_key
    }

    pub fn hashing_algorithm(&self) -> &str {
        &self.hashing_algorithm
    }

    pub fn code_length(&self) -> u32 {
        self.code_length
    }

    pub fn code_interval_seconds(&self) -> u32 {
        self.code_interval_seconds
    }

    pub fn enrollment_deadline(&self) -> SystemTime {
        self.enrollment_deadline
    }

    pub fn qr_code_url(&self, account_name: Option<&str>, issuer: Option<&str>) -> String {
        let auth = self.auth.upgrade();
        let default_account = account_name
            .filter(|name| !name.is_empty())
            .map(|value| value.to_string())
            .or_else(|| {
                auth.as_ref()
                    .and_then(|auth| auth.current_user())
                    .and_then(|user| user.info().email.clone())
            })
            .unwrap_or_else(|| "unknownuser".into());
        let default_issuer = issuer
            .filter(|name| !name.is_empty())
            .map(|value| value.to_string())
            .or_else(|| auth.as_ref().map(|auth| auth.app().name().to_string()))
            .unwrap_or_else(|| "firebase".into());
        let encoded_issuer: String = byte_serialize(default_issuer.as_bytes()).collect();
        format!(
            "otpauth://totp/{}:{}?secret={}&issuer={}&algorithm={}&digits={}",
            default_issuer,
            default_account,
            self.secret_key,
            encoded_issuer,
            self.hashing_algorithm,
            self.code_length
        )
    }

    pub(crate) fn session_info(&self) -> &str {
        &self.session_info
    }
}

#[derive(Clone, Debug)]
pub struct TotpMultiFactorAssertion {
    secret: Option<TotpSecret>,
    enrollment_id: Option<String>,
    otp: String,
}

impl TotpMultiFactorAssertion {
    pub(crate) fn for_enrollment(secret: TotpSecret, otp: impl Into<String>) -> Self {
        Self {
            secret: Some(secret),
            enrollment_id: None,
            otp: otp.into(),
        }
    }

    pub(crate) fn for_sign_in(enrollment_id: impl Into<String>, otp: impl Into<String>) -> Self {
        Self {
            secret: None,
            enrollment_id: Some(enrollment_id.into()),
            otp: otp.into(),
        }
    }

    pub(crate) fn otp(&self) -> &str {
        &self.otp
    }

    pub(crate) fn secret(&self) -> Option<&TotpSecret> {
        self.secret.as_ref()
    }

    pub(crate) fn enrollment_id(&self) -> Option<&str> {
        self.enrollment_id.as_deref()
    }
}

#[derive(Clone, Debug)]
pub enum WebAuthnAssertionKind {
    SignIn {
        enrollment_id: String,
        response: WebAuthnAssertionResponse,
    },
    Enrollment {
        attestation: WebAuthnAttestationResponse,
    },
}

#[derive(Clone, Debug)]
pub struct WebAuthnMultiFactorAssertion {
    kind: WebAuthnAssertionKind,
}

impl WebAuthnMultiFactorAssertion {
    pub fn for_sign_in(
        enrollment_id: impl Into<String>,
        response: WebAuthnAssertionResponse,
    ) -> Self {
        Self {
            kind: WebAuthnAssertionKind::SignIn {
                enrollment_id: enrollment_id.into(),
                response,
            },
        }
    }

    pub fn for_enrollment(attestation: WebAuthnAttestationResponse) -> Self {
        Self {
            kind: WebAuthnAssertionKind::Enrollment { attestation },
        }
    }

    pub fn enrollment_id(&self) -> Option<&str> {
        match &self.kind {
            WebAuthnAssertionKind::SignIn { enrollment_id, .. } => Some(enrollment_id),
            WebAuthnAssertionKind::Enrollment { .. } => None,
        }
    }

    pub fn response(&self) -> Option<&WebAuthnAssertionResponse> {
        match &self.kind {
            WebAuthnAssertionKind::SignIn { response, .. } => Some(response),
            WebAuthnAssertionKind::Enrollment { .. } => None,
        }
    }

    pub fn into_sign_in(self) -> Option<(String, WebAuthnAssertionResponse)> {
        match self.kind {
            WebAuthnAssertionKind::SignIn {
                enrollment_id,
                response,
            } => Some((enrollment_id, response)),
            _ => None,
        }
    }

    pub fn attestation(&self) -> Option<&WebAuthnAttestationResponse> {
        match &self.kind {
            WebAuthnAssertionKind::Enrollment { attestation } => Some(attestation),
            _ => None,
        }
    }

    pub fn into_attestation(self) -> Option<WebAuthnAttestationResponse> {
        match self.kind {
            WebAuthnAssertionKind::Enrollment { attestation } => Some(attestation),
            _ => None,
        }
    }
}

/// A multi-factor assertion that can be resolved to complete sign-in.
///
/// Mirrors the behaviour of the JavaScript `MultiFactorAssertion` found in
/// `packages/auth/src/mfa/mfa_assertion.ts`.
#[derive(Clone, Debug)]
pub enum MultiFactorAssertion {
    Phone(PhoneMultiFactorAssertion),
    Totp(TotpMultiFactorAssertion),
    WebAuthn(WebAuthnMultiFactorAssertion),
}

impl MultiFactorAssertion {
    /// Returns the identifier of the underlying second factor.
    pub fn factor_id(&self) -> &'static str {
        match self {
            MultiFactorAssertion::Phone(_) => PHONE_PROVIDER_ID,
            MultiFactorAssertion::Totp(_) => "totp",
            MultiFactorAssertion::WebAuthn(_) => WEBAUTHN_FACTOR_ID,
        }
    }

    pub(crate) fn from_phone_credential(credential: PhoneAuthCredential) -> Self {
        MultiFactorAssertion::Phone(PhoneMultiFactorAssertion::new(credential))
    }

    pub(crate) fn from_totp_enrollment(secret: TotpSecret, otp: impl Into<String>) -> Self {
        MultiFactorAssertion::Totp(TotpMultiFactorAssertion::for_enrollment(secret, otp))
    }

    pub(crate) fn from_totp_sign_in(
        enrollment_id: impl Into<String>,
        otp: impl Into<String>,
    ) -> Self {
        MultiFactorAssertion::Totp(TotpMultiFactorAssertion::for_sign_in(enrollment_id, otp))
    }

    pub(crate) fn from_passkey(
        enrollment_id: impl Into<String>,
        response: WebAuthnAssertionResponse,
    ) -> Self {
        MultiFactorAssertion::WebAuthn(WebAuthnMultiFactorAssertion::for_sign_in(
            enrollment_id,
            response,
        ))
    }
}

/// Builder for time-based one-time password multi-factor assertions.
pub struct TotpMultiFactorGenerator;

impl TotpMultiFactorGenerator {
    pub fn assertion_for_enrollment(
        secret: TotpSecret,
        otp: impl Into<String>,
    ) -> MultiFactorAssertion {
        MultiFactorAssertion::from_totp_enrollment(secret, otp)
    }

    pub fn assertion_for_sign_in(
        enrollment_id: impl Into<String>,
        otp: impl Into<String>,
    ) -> MultiFactorAssertion {
        MultiFactorAssertion::from_totp_sign_in(enrollment_id, otp)
    }

    pub async fn generate_secret(
        auth: &FirebaseAuth,
        session: &MultiFactorSession,
    ) -> AuthResult<TotpSecret> {
        let inner = auth.inner_arc();
        inner.start_totp_mfa_enrollment(session).await
    }

    pub const FACTOR_ID: &'static str = "totp";
}

pub const WEBAUTHN_FACTOR_ID: &str = "webauthn";

/// The transport mechanisms declared for a WebAuthn credential.
///
/// Mirrors the identifiers returned by the WebAuthn browser APIs and exposed by the Firebase
/// JS SDK (`packages/auth/src/model/public_types.ts`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum WebAuthnTransport {
    Usb,
    Nfc,
    Ble,
    Internal,
    Cable,
    Hybrid,
    /// A transport identifier that is not yet known to this SDK.
    Unknown(String),
}

impl WebAuthnTransport {
    fn from_raw(value: &str) -> Self {
        match value {
            "usb" => WebAuthnTransport::Usb,
            "nfc" => WebAuthnTransport::Nfc,
            "ble" => WebAuthnTransport::Ble,
            "internal" => WebAuthnTransport::Internal,
            "cable" => WebAuthnTransport::Cable,
            "hybrid" => WebAuthnTransport::Hybrid,
            other => WebAuthnTransport::Unknown(other.to_string()),
        }
    }

    /// Returns the string identifier used by the underlying WebAuthn API.
    pub fn as_str(&self) -> &str {
        match self {
            WebAuthnTransport::Usb => "usb",
            WebAuthnTransport::Nfc => "nfc",
            WebAuthnTransport::Ble => "ble",
            WebAuthnTransport::Internal => "internal",
            WebAuthnTransport::Cable => "cable",
            WebAuthnTransport::Hybrid => "hybrid",
            WebAuthnTransport::Unknown(value) => value.as_str(),
        }
    }
}

impl fmt::Display for WebAuthnTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

fn parse_transports(value: &Value) -> Vec<WebAuthnTransport> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|transport| transport.as_str())
                .map(WebAuthnTransport::from_raw)
                .collect()
        })
        .unwrap_or_default()
}

/// Descriptor describing a credential that can satisfy a WebAuthn challenge.
///
/// Follows the shape returned by the Identity Toolkit API and exposed by the modular JS SDK
/// (`PublicKeyCredentialDescriptor` in
/// `packages/auth/src/model/public_types.ts`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebAuthnCredentialDescriptor {
    id: String,
    cred_type: String,
    transports: Vec<WebAuthnTransport>,
}

impl WebAuthnCredentialDescriptor {
    fn from_json(value: &Value) -> Option<Self> {
        let id = value.get("id")?.as_str()?.to_string();
        let cred_type = value
            .get("type")
            .and_then(|candidate| candidate.as_str())
            .unwrap_or("public-key")
            .to_string();
        let transports = value
            .get("transports")
            .map(parse_transports)
            .unwrap_or_default();
        Some(Self {
            id,
            cred_type,
            transports,
        })
    }

    /// Returns the base64url encoded credential identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the credential type (usually `"public-key"`).
    pub fn credential_type(&self) -> &str {
        &self.cred_type
    }

    /// Returns the transports declared for this credential.
    pub fn transports(&self) -> &[WebAuthnTransport] {
        &self.transports
    }
}

pub struct WebAuthnMultiFactorGenerator;

impl WebAuthnMultiFactorGenerator {
    pub fn assertion_for_sign_in(
        enrollment_id: impl Into<String>,
        response: WebAuthnAssertionResponse,
    ) -> MultiFactorAssertion {
        MultiFactorAssertion::from_passkey(enrollment_id, response)
    }

    pub fn assertion_for_enrollment(
        attestation: WebAuthnAttestationResponse,
    ) -> MultiFactorAssertion {
        MultiFactorAssertion::WebAuthn(WebAuthnMultiFactorAssertion::for_enrollment(attestation))
    }
}

#[derive(Clone, Debug)]
pub struct WebAuthnSignInChallenge {
    payload: Value,
}

impl WebAuthnSignInChallenge {
    pub fn challenge(&self) -> Option<&str> {
        self.payload
            .get("challenge")
            .and_then(|value| value.as_str())
    }

    pub fn rp_id(&self) -> Option<&str> {
        self.payload.get("rpId").and_then(|value| value.as_str())
    }

    pub fn user_handle(&self) -> Option<&str> {
        self.payload
            .get("userHandle")
            .and_then(|value| value.as_str())
    }

    /// Returns the credential descriptors that can satisfy this challenge.
    pub fn allow_credentials(&self) -> Vec<WebAuthnCredentialDescriptor> {
        self.payload
            .get("allowCredentials")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(WebAuthnCredentialDescriptor::from_json)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn as_raw(&self) -> &Value {
        &self.payload
    }

    pub fn into_raw(self) -> Value {
        self.payload
    }

    pub fn from_value(value: Value) -> AuthResult<Self> {
        if value
            .get("challenge")
            .and_then(|candidate| candidate.as_str())
            .is_none()
        {
            return Err(AuthError::InvalidCredential(
                "WebAuthn sign-in challenge is missing a challenge value".into(),
            ));
        }
        Ok(Self { payload: value })
    }
}

#[derive(Clone, Debug)]
pub struct WebAuthnEnrollmentChallenge {
    payload: Value,
}

impl WebAuthnEnrollmentChallenge {
    pub fn challenge(&self) -> Option<&str> {
        self.payload
            .get("challenge")
            .and_then(|value| value.as_str())
    }

    pub fn rp_id(&self) -> Option<&str> {
        self.payload.get("rpId").and_then(|value| value.as_str())
    }

    pub fn user_name(&self) -> Option<&str> {
        self.payload
            .get("user")
            .and_then(|user| user.get("name"))
            .and_then(|value| value.as_str())
    }

    pub fn user_id(&self) -> Option<&[u8]> {
        self.payload
            .get("user")
            .and_then(|user| user.get("id"))
            .and_then(|value| value.as_str())
            .map(|encoded| encoded.as_bytes())
    }

    pub fn as_raw(&self) -> &Value {
        &self.payload
    }

    pub fn into_raw(self) -> Value {
        self.payload
    }

    pub fn from_value(value: Value) -> AuthResult<Self> {
        if value
            .get("challenge")
            .and_then(|candidate| candidate.as_str())
            .is_none()
        {
            return Err(AuthError::InvalidCredential(
                "WebAuthn enrollment challenge is missing a challenge value".into(),
            ));
        }
        Ok(Self { payload: value })
    }
}

#[derive(Clone, Debug)]
pub struct WebAuthnAssertionResponse {
    payload: Value,
}

impl WebAuthnAssertionResponse {
    pub fn credential_id(&self) -> Option<&str> {
        self.payload
            .get("credentialId")
            .and_then(|value| value.as_str())
    }

    pub fn client_data_json(&self) -> Option<&str> {
        self.payload
            .get("clientDataJSON")
            .and_then(|value| value.as_str())
    }

    /// Returns the authenticator data associated with the assertion response.
    pub fn authenticator_data(&self) -> Option<&str> {
        self.payload
            .get("authenticatorData")
            .and_then(|value| value.as_str())
    }

    /// Returns the raw signature produced by the authenticator, base64url encoded.
    pub fn signature(&self) -> Option<&str> {
        self.payload
            .get("signature")
            .and_then(|value| value.as_str())
    }

    /// Returns the optional user handle provided by the authenticator.
    pub fn user_handle(&self) -> Option<&str> {
        self.payload
            .get("userHandle")
            .and_then(|value| value.as_str())
    }

    pub fn as_raw(&self) -> &Value {
        &self.payload
    }

    pub fn into_raw(self) -> Value {
        self.payload
    }
}

#[derive(Clone, Debug)]
pub struct WebAuthnAttestationResponse {
    payload: Value,
}

impl WebAuthnAttestationResponse {
    pub fn credential_id(&self) -> Option<&str> {
        self.payload
            .get("credentialId")
            .and_then(|value| value.as_str())
    }

    pub fn client_data_json(&self) -> Option<&str> {
        self.payload
            .get("clientDataJSON")
            .and_then(|value| value.as_str())
    }

    /// Returns the attestation object emitted by the authenticator, base64url encoded.
    pub fn attestation_object(&self) -> Option<&str> {
        self.payload
            .get("attestationObject")
            .and_then(|value| value.as_str())
    }

    /// Returns the credential public key when provided by the platform authenticator.
    pub fn credential_public_key(&self) -> Option<&str> {
        self.payload
            .get("credentialPublicKey")
            .and_then(|value| value.as_str())
    }

    /// Returns the transports declared by the authenticator for the newly enrolled credential.
    pub fn transports(&self) -> Vec<WebAuthnTransport> {
        self.payload
            .get("transports")
            .map(parse_transports)
            .unwrap_or_default()
    }

    pub fn as_raw(&self) -> &Value {
        &self.payload
    }

    pub fn into_raw(self) -> Value {
        self.payload
    }
}

impl TryFrom<Value> for WebAuthnAttestationResponse {
    type Error = AuthError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let credential_present = value
            .get("credentialId")
            .and_then(|candidate| candidate.as_str())
            .is_some();
        let attestation_present = value
            .get("attestationObject")
            .and_then(|candidate| candidate.as_str())
            .is_some();
        let client_data_present = value
            .get("clientDataJSON")
            .and_then(|candidate| candidate.as_str())
            .is_some();

        if credential_present && attestation_present && client_data_present {
            Ok(Self { payload: value })
        } else {
            Err(AuthError::InvalidCredential(
                "WebAuthn registration payload missing required fields".into(),
            ))
        }
    }
}

impl TryFrom<Value> for WebAuthnAssertionResponse {
    type Error = AuthError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let credential_present = value
            .get("credentialId")
            .and_then(|candidate| candidate.as_str())
            .is_some();
        let client_data_present = value
            .get("clientDataJSON")
            .and_then(|candidate| candidate.as_str())
            .is_some();

        if credential_present && client_data_present {
            Ok(Self { payload: value })
        } else {
            Err(AuthError::InvalidCredential(
                "WebAuthn verification payload missing credentialId or clientDataJSON".into(),
            ))
        }
    }
}

#[derive(Clone, Debug)]
pub struct MultiFactorError {
    operation: MultiFactorOperation,
    hints: Vec<MultiFactorInfo>,
    session: MultiFactorSession,
    context: Arc<MultiFactorSignInContext>,
    user: Option<Arc<User>>,
}

impl MultiFactorError {
    pub(crate) fn new(
        operation: MultiFactorOperation,
        session: MultiFactorSession,
        hints: Vec<MultiFactorInfo>,
        context: MultiFactorSignInContext,
        user: Option<Arc<User>>,
    ) -> Self {
        Self {
            operation,
            hints,
            session,
            context: Arc::new(context),
            user,
        }
    }

    /// Returns the factors that can satisfy the pending challenge.
    pub fn hints(&self) -> &[MultiFactorInfo] {
        &self.hints
    }

    /// Returns the captured multi-factor session information.
    pub fn session(&self) -> &MultiFactorSession {
        &self.session
    }

    /// Returns the operation that triggered the multi-factor requirement.
    pub fn operation(&self) -> MultiFactorOperation {
        self.operation
    }

    pub(crate) fn context(&self) -> Arc<MultiFactorSignInContext> {
        Arc::clone(&self.context)
    }

    pub(crate) fn user(&self) -> Option<Arc<User>> {
        self.user.clone()
    }
}

impl fmt::Display for MultiFactorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.operation {
            MultiFactorOperation::SignIn => write!(f, "Multi-factor sign-in required"),
            MultiFactorOperation::Reauthenticate => {
                write!(f, "Multi-factor reauthentication required")
            }
            MultiFactorOperation::Link => {
                write!(f, "Multi-factor linking required")
            }
        }
    }
}

#[derive(Clone)]
pub struct MultiFactorResolver {
    auth: Arc<Auth>,
    hints: Vec<MultiFactorInfo>,
    session: MultiFactorSession,
    operation: MultiFactorOperation,
    context: Arc<MultiFactorSignInContext>,
    _user: Option<Arc<User>>,
}

impl MultiFactorResolver {
    pub(crate) fn from_error(auth: Arc<Auth>, error: MultiFactorError) -> Self {
        Self {
            hints: error.hints.clone(),
            session: error.session.clone(),
            operation: error.operation(),
            context: error.context(),
            _user: error.user(),
            auth,
        }
    }

    /// Returns the list of factor hints that can satisfy the pending challenge.
    pub fn hints(&self) -> &[MultiFactorInfo] {
        &self.hints
    }

    /// Returns the session associated with the pending multi-factor challenge.
    pub fn session(&self) -> &MultiFactorSession {
        &self.session
    }

    /// Initiates a phone-based multi-factor challenge for the provided hint.
    pub async fn send_phone_sign_in_code(
        &self,
        hint: &MultiFactorInfo,
        verifier: Arc<dyn ApplicationVerifier>,
    ) -> AuthResult<String> {
        let pending = self.session.pending_credential().ok_or_else(|| {
            AuthError::InvalidCredential(
                "Multi-factor session is not valid for challenge resolution".into(),
            )
        })?;

        self.auth
            .start_phone_multi_factor_sign_in(pending, &hint.uid, verifier)
            .await
    }

    /// Initiates a passkey/WebAuthn challenge for the provided factor hint.
    pub async fn start_passkey_sign_in(
        &self,
        hint: &MultiFactorInfo,
    ) -> AuthResult<WebAuthnSignInChallenge> {
        if hint.factor_id != WEBAUTHN_FACTOR_ID {
            return Err(AuthError::InvalidCredential(
                "Hint does not reference a WebAuthn factor".into(),
            ));
        }

        let pending = self.session.pending_credential().ok_or_else(|| {
            AuthError::InvalidCredential(
                "Multi-factor session is not valid for challenge resolution".into(),
            )
        })?;

        self.auth
            .start_passkey_multi_factor_sign_in(pending, &hint.uid)
            .await
    }

    /// Resolves the pending multi-factor challenge using the supplied assertion.
    pub async fn resolve_sign_in(
        &self,
        assertion: MultiFactorAssertion,
    ) -> AuthResult<UserCredential> {
        let pending = self.session.pending_credential().ok_or_else(|| {
            AuthError::InvalidCredential(
                "Multi-factor session is not valid for challenge resolution".into(),
            )
        })?;

        match assertion {
            MultiFactorAssertion::Phone(assertion) => {
                let verification_id = assertion.credential().verification_id();
                let verification_code = assertion.credential().verification_code();

                self.auth
                    .finalize_phone_multi_factor_sign_in(
                        pending,
                        verification_id,
                        verification_code,
                        Arc::clone(&self.context),
                        self.operation,
                    )
                    .await
            }
            MultiFactorAssertion::Totp(assertion) => {
                let enrollment_id = assertion.enrollment_id().ok_or_else(|| {
                    AuthError::InvalidCredential(
                        "TOTP assertions require an enrollment identifier".into(),
                    )
                })?;

                self.auth
                    .finalize_totp_multi_factor_sign_in(
                        pending,
                        enrollment_id,
                        assertion.otp(),
                        Arc::clone(&self.context),
                        self.operation,
                    )
                    .await
            }
            MultiFactorAssertion::WebAuthn(assertion) => {
                let (enrollment_id, response) = assertion.into_sign_in().ok_or_else(|| {
                    AuthError::InvalidCredential(
                        "WebAuthn assertion is not valid for sign-in".to_string(),
                    )
                })?;
                self.auth
                    .finalize_passkey_multi_factor_sign_in(
                        pending,
                        &enrollment_id,
                        response,
                        Arc::clone(&self.context),
                        self.operation,
                    )
                    .await
            }
        }
    }
}

#[derive(Clone)]
pub struct MultiFactorUser {
    auth: Arc<Auth>,
}

impl MultiFactorUser {
    pub(crate) fn new(auth: Arc<Auth>) -> Self {
        Self { auth }
    }

    /// Returns the list of enrolled multi-factor authenticators.
    pub async fn enrolled_factors(&self) -> AuthResult<Vec<MultiFactorInfo>> {
        self.auth.fetch_enrolled_factors().await
    }

    /// Requests a multi-factor session for subsequent operations.
    pub async fn get_session(&self) -> AuthResult<MultiFactorSession> {
        self.auth.multi_factor_session().await
    }

    /// Generates a TOTP enrollment secret for the provided session.
    pub async fn generate_totp_secret(
        &self,
        session: &MultiFactorSession,
    ) -> AuthResult<TotpSecret> {
        self.auth.start_totp_mfa_enrollment(session).await
    }

    /// Completes enrollment using a multi-factor assertion (e.g. TOTP).
    pub async fn enroll(
        &self,
        session: &MultiFactorSession,
        assertion: MultiFactorAssertion,
        display_name: Option<&str>,
    ) -> AuthResult<UserCredential> {
        match assertion {
            MultiFactorAssertion::Totp(assertion) => {
                if session.session_type() != MultiFactorSessionType::Enrollment {
                    return Err(AuthError::InvalidCredential(
                        "TOTP enrollment requires an enrollment session".into(),
                    ));
                }
                let id_token = session.id_token().ok_or_else(|| {
                    AuthError::InvalidCredential("Missing ID token for enrollment".into())
                })?;
                let secret = assertion.secret().ok_or_else(|| {
                    AuthError::InvalidCredential(
                        "TOTP enrollment assertions require a generated secret".into(),
                    )
                })?;
                self.auth
                    .complete_totp_mfa_enrollment(id_token, secret, assertion.otp(), display_name)
                    .await
            }
            MultiFactorAssertion::WebAuthn(assertion) => {
                if session.session_type() != MultiFactorSessionType::Enrollment {
                    return Err(AuthError::InvalidCredential(
                        "WebAuthn enrollment requires an enrollment session".into(),
                    ));
                }

                let attestation = assertion.into_attestation().ok_or_else(|| {
                    AuthError::InvalidCredential(
                        "WebAuthn enrollment assertions require an attestation payload".to_string(),
                    )
                })?;

                self.auth
                    .complete_passkey_mfa_enrollment(session, attestation, display_name)
                    .await
            }
            _ => Err(AuthError::NotImplemented(
                "Only TOTP and WebAuthn assertions are supported via MultiFactorUser::enroll",
            )),
        }
    }

    /// Starts a WebAuthn/passkey enrollment challenge.
    pub async fn start_passkey_enrollment(
        &self,
        session: &MultiFactorSession,
    ) -> AuthResult<WebAuthnEnrollmentChallenge> {
        self.auth.start_passkey_mfa_enrollment(session).await
    }

    /// Starts phone number enrollment by sending a verification SMS.
    pub async fn enroll_phone_number(
        &self,
        phone_number: &str,
        verifier: Arc<dyn ApplicationVerifier>,
        display_name: Option<&str>,
    ) -> AuthResult<ConfirmationResult> {
        self.auth
            .start_phone_mfa_enrollment(phone_number, verifier, display_name)
            .await
    }

    /// Removes an enrolled multi-factor authenticator.
    pub async fn unenroll(&self, factor_uid: &str) -> AuthResult<()> {
        self.auth.withdraw_multi_factor(factor_uid).await
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

    pub(crate) fn inner_arc(&self) -> Arc<Auth> {
        self.inner.clone()
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

/// Returns a [`MultiFactorResolver`] that can be used to complete a pending multi-factor flow.
///
/// Mirrors the JavaScript helper exported from `packages/auth/src/mfa/mfa_resolver.ts`.
pub fn get_multi_factor_resolver(
    auth: &FirebaseAuth,
    error: &AuthError,
) -> AuthResult<MultiFactorResolver> {
    match error {
        AuthError::MultiFactorRequired(mfa_error) => Ok(MultiFactorResolver::from_error(
            auth.inner_arc(),
            mfa_error.clone(),
        )),
        _ => Err(AuthError::InvalidCredential(
            "The supplied error does not contain multi-factor context".into(),
        )),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::error::AuthError;
    use serde_json::json;

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

    #[test]
    fn webauthn_sign_in_challenge_accessors() {
        let payload = json!({
            "challenge": "ABC",
            "rpId": "example.com",
            "userHandle": "user-handle",
            "allowCredentials": [
                {
                    "type": "public-key",
                    "id": "cred-1",
                    "transports": ["usb", "internal"]
                },
                {
                    "type": "public-key",
                    "id": "cred-2"
                }
            ]
        });

        let challenge = WebAuthnSignInChallenge::from_value(payload).expect("valid challenge");
        let allow = challenge.allow_credentials();
        assert_eq!(allow.len(), 2);
        assert_eq!(allow[0].id(), "cred-1");
        assert_eq!(allow[0].credential_type(), "public-key");
        assert_eq!(
            allow[0].transports(),
            &[WebAuthnTransport::Usb, WebAuthnTransport::Internal]
        );
        assert_eq!(allow[1].id(), "cred-2");
        assert!(allow[1].transports().is_empty());
        assert_eq!(challenge.user_handle(), Some("user-handle"));
    }

    #[test]
    fn webauthn_attestation_response_accessors() {
        let payload = json!({
            "credentialId": "cred-123",
            "clientDataJSON": "BASE64CLIENT",
            "attestationObject": "ATTEST",
            "credentialPublicKey": "PUBKEY",
            "transports": ["nfc", "unknown"]
        });

        let response = WebAuthnAttestationResponse::try_from(payload).expect("attestation");
        assert_eq!(response.credential_id(), Some("cred-123"));
        assert_eq!(response.client_data_json(), Some("BASE64CLIENT"));
        assert_eq!(response.attestation_object(), Some("ATTEST"));
        assert_eq!(response.credential_public_key(), Some("PUBKEY"));
        let transports = response.transports();
        assert_eq!(transports.len(), 2);
        assert_eq!(transports[0], WebAuthnTransport::Nfc);
        assert_eq!(transports[1].as_str(), "unknown");
    }

    #[test]
    fn webauthn_assertion_response_accessors() {
        let payload = json!({
            "credentialId": "cred-abc",
            "clientDataJSON": "CLIENT",
            "authenticatorData": "AUTH_DATA",
            "signature": "SIG",
            "userHandle": "USER"
        });

        let response = WebAuthnAssertionResponse::try_from(payload).expect("assertion");
        assert_eq!(response.credential_id(), Some("cred-abc"));
        assert_eq!(response.client_data_json(), Some("CLIENT"));
        assert_eq!(response.authenticator_data(), Some("AUTH_DATA"));
        assert_eq!(response.signature(), Some("SIG"));
        assert_eq!(response.user_handle(), Some("USER"));
    }
}
