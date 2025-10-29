use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::auth::error::{map_mfa_error_code, AuthError, AuthResult};

fn endpoint_url(base: &str, path: &str, api_key: &str) -> String {
    format!("{}/{}?key={}", base.trim_end_matches('/'), path, api_key)
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: Option<ErrorInfo>,
}

#[derive(Debug, Deserialize)]
struct ErrorInfo {
    message: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct PhoneEnrollmentInfo {
    #[serde(rename = "phoneNumber")]
    pub phone_number: String,
    #[serde(rename = "recaptchaToken", skip_serializing_if = "Option::is_none")]
    pub recaptcha_token: Option<String>,
    #[serde(rename = "captchaResponse", skip_serializing_if = "Option::is_none")]
    pub captcha_response: Option<String>,
    #[serde(rename = "clientType", skip_serializing_if = "Option::is_none")]
    pub client_type: Option<String>,
    #[serde(rename = "recaptchaVersion", skip_serializing_if = "Option::is_none")]
    pub recaptcha_version: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct StartPhoneMfaEnrollmentRequest {
    #[serde(rename = "idToken")]
    pub id_token: String,
    #[serde(rename = "phoneEnrollmentInfo")]
    pub phone_enrollment_info: PhoneEnrollmentInfo,
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StartPhoneMfaEnrollmentResponse {
    #[serde(rename = "phoneSessionInfo")]
    pub phone_session_info: PhoneSessionInfo,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PhoneSessionInfo {
    #[serde(rename = "sessionInfo")]
    pub session_info: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct StartTotpMfaEnrollmentRequest {
    #[serde(rename = "idToken")]
    pub id_token: String,
    #[serde(rename = "totpEnrollmentInfo")]
    pub totp_enrollment_info: Value,
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TotpSessionInfo {
    #[serde(rename = "sharedSecretKey")]
    pub shared_secret_key: String,
    #[serde(rename = "verificationCodeLength")]
    pub verification_code_length: u32,
    #[serde(rename = "hashingAlgorithm")]
    pub hashing_algorithm: String,
    #[serde(rename = "periodSec")]
    pub period_sec: u32,
    #[serde(rename = "sessionInfo")]
    pub session_info: String,
    #[serde(rename = "finalizeEnrollmentTime")]
    pub finalize_enrollment_time: i64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StartTotpMfaEnrollmentResponse {
    #[serde(rename = "totpSessionInfo")]
    pub totp_session_info: TotpSessionInfo,
}

#[derive(Debug, Serialize, Clone)]
pub struct FinalizePhoneMfaEnrollmentRequest {
    #[serde(rename = "idToken")]
    pub id_token: String,
    #[serde(rename = "phoneVerificationInfo")]
    pub phone_verification_info: PhoneVerificationInfo,
    #[serde(rename = "displayName", skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct PhoneVerificationInfo {
    #[serde(rename = "sessionInfo")]
    pub session_info: String,
    #[serde(rename = "code")]
    pub code: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FinalizeMfaEnrollmentResponse {
    #[serde(rename = "idToken")]
    pub id_token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct TotpVerificationInfo {
    #[serde(rename = "sessionInfo")]
    pub session_info: String,
    #[serde(rename = "verificationCode")]
    pub verification_code: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct FinalizeTotpMfaEnrollmentRequest {
    #[serde(rename = "idToken")]
    pub id_token: String,
    #[serde(rename = "totpVerificationInfo")]
    pub totp_verification_info: TotpVerificationInfo,
    #[serde(rename = "displayName", skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FinalizeTotpMfaEnrollmentResponse {
    #[serde(rename = "idToken")]
    pub id_token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct WithdrawMfaRequest {
    #[serde(rename = "idToken")]
    pub id_token: String,
    #[serde(rename = "mfaEnrollmentId")]
    pub mfa_enrollment_id: String,
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WithdrawMfaResponse {
    #[serde(rename = "idToken")]
    pub id_token: Option<String>,
    #[serde(rename = "refreshToken")]
    pub refresh_token: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct StartPhoneMfaSignInRequest {
    #[serde(rename = "mfaPendingCredential")]
    pub mfa_pending_credential: String,
    #[serde(rename = "mfaEnrollmentId")]
    pub mfa_enrollment_id: String,
    #[serde(rename = "phoneSignInInfo")]
    pub phone_sign_in_info: PhoneSignInInfo,
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct PhoneSignInInfo {
    #[serde(rename = "recaptchaToken", skip_serializing_if = "Option::is_none")]
    pub recaptcha_token: Option<String>,
    #[serde(rename = "captchaResponse", skip_serializing_if = "Option::is_none")]
    pub captcha_response: Option<String>,
    #[serde(rename = "clientType", skip_serializing_if = "Option::is_none")]
    pub client_type: Option<String>,
    #[serde(rename = "recaptchaVersion", skip_serializing_if = "Option::is_none")]
    pub recaptcha_version: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StartPhoneMfaSignInResponse {
    #[serde(rename = "phoneResponseInfo")]
    pub phone_response_info: PhoneSessionInfo,
}

#[derive(Debug, Serialize, Clone)]
pub struct StartPasskeyMfaEnrollmentRequest {
    #[serde(rename = "idToken")]
    pub id_token: String,
    #[serde(rename = "webauthnEnrollmentInfo")]
    pub webauthn_enrollment_info: Value,
    #[serde(rename = "displayName", skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StartPasskeyMfaEnrollmentResponse {
    #[serde(rename = "webauthnEnrollmentInfo")]
    pub webauthn_enrollment_info: Value,
}

#[derive(Debug, Serialize, Clone)]
pub struct StartPasskeyMfaSignInRequest {
    #[serde(rename = "mfaPendingCredential")]
    pub mfa_pending_credential: String,
    #[serde(rename = "mfaEnrollmentId")]
    pub mfa_enrollment_id: String,
    #[serde(rename = "webauthnSignInInfo", skip_serializing_if = "Option::is_none")]
    pub webauthn_sign_in_info: Option<Value>,
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StartPasskeyMfaSignInResponse {
    #[serde(rename = "webauthnSignInInfo")]
    pub webauthn_sign_in_info: Value,
}

#[derive(Debug, Serialize, Clone)]
pub struct FinalizePhoneMfaSignInRequest {
    #[serde(rename = "mfaPendingCredential")]
    pub mfa_pending_credential: String,
    #[serde(rename = "phoneVerificationInfo")]
    pub phone_verification_info: PhoneVerificationInfo,
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FinalizeMfaSignInResponse {
    #[serde(rename = "idToken")]
    pub id_token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct WebAuthnVerificationInfo {
    #[serde(flatten)]
    pub payload: Value,
}

#[derive(Debug, Serialize, Clone)]
pub struct TotpSignInVerificationInfo {
    #[serde(rename = "verificationCode")]
    pub verification_code: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct FinalizeTotpMfaSignInRequest {
    #[serde(rename = "mfaPendingCredential")]
    pub mfa_pending_credential: String,
    #[serde(rename = "mfaEnrollmentId")]
    pub mfa_enrollment_id: String,
    #[serde(rename = "totpVerificationInfo")]
    pub totp_verification_info: TotpSignInVerificationInfo,
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FinalizeTotpMfaSignInResponse {
    #[serde(rename = "idToken")]
    pub id_token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct FinalizePasskeyMfaEnrollmentRequest {
    #[serde(rename = "idToken")]
    pub id_token: String,
    #[serde(rename = "webauthnVerificationInfo")]
    pub webauthn_verification_info: WebAuthnVerificationInfo,
    #[serde(rename = "displayName", skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FinalizePasskeyMfaEnrollmentResponse {
    #[serde(rename = "idToken")]
    pub id_token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct FinalizePasskeyMfaSignInRequest {
    #[serde(rename = "mfaPendingCredential")]
    pub mfa_pending_credential: String,
    #[serde(rename = "mfaEnrollmentId", skip_serializing_if = "Option::is_none")]
    pub mfa_enrollment_id: Option<String>,
    #[serde(rename = "webauthnVerificationInfo")]
    pub webauthn_verification_info: WebAuthnVerificationInfo,
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

pub async fn start_phone_mfa_enrollment(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    request: &StartPhoneMfaEnrollmentRequest,
) -> AuthResult<StartPhoneMfaEnrollmentResponse> {
    post_json(
        client,
        endpoint,
        "accounts/mfaEnrollment:start",
        api_key,
        request,
    )
    .await
}

pub async fn start_totp_mfa_enrollment(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    request: &StartTotpMfaEnrollmentRequest,
) -> AuthResult<StartTotpMfaEnrollmentResponse> {
    post_json(
        client,
        endpoint,
        "accounts/mfaEnrollment:start",
        api_key,
        request,
    )
    .await
}

pub async fn start_passkey_mfa_enrollment(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    request: &StartPasskeyMfaEnrollmentRequest,
) -> AuthResult<StartPasskeyMfaEnrollmentResponse> {
    post_json(
        client,
        endpoint,
        "accounts/mfaEnrollment:start",
        api_key,
        request,
    )
    .await
}

pub async fn finalize_phone_mfa_enrollment(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    request: &FinalizePhoneMfaEnrollmentRequest,
) -> AuthResult<FinalizeMfaEnrollmentResponse> {
    post_json(
        client,
        endpoint,
        "accounts/mfaEnrollment:finalize",
        api_key,
        request,
    )
    .await
}

pub async fn finalize_totp_mfa_enrollment(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    request: &FinalizeTotpMfaEnrollmentRequest,
) -> AuthResult<FinalizeTotpMfaEnrollmentResponse> {
    post_json(
        client,
        endpoint,
        "accounts/mfaEnrollment:finalize",
        api_key,
        request,
    )
    .await
}

pub async fn finalize_passkey_mfa_enrollment(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    request: &FinalizePasskeyMfaEnrollmentRequest,
) -> AuthResult<FinalizePasskeyMfaEnrollmentResponse> {
    post_json(
        client,
        endpoint,
        "accounts/mfaEnrollment:finalize",
        api_key,
        request,
    )
    .await
}

pub async fn withdraw_mfa(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    request: &WithdrawMfaRequest,
) -> AuthResult<WithdrawMfaResponse> {
    post_json(
        client,
        endpoint,
        "accounts/mfaEnrollment:withdraw",
        api_key,
        request,
    )
    .await
}

pub async fn start_phone_mfa_sign_in(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    request: &StartPhoneMfaSignInRequest,
) -> AuthResult<StartPhoneMfaSignInResponse> {
    post_json(
        client,
        endpoint,
        "accounts/mfaSignIn:start",
        api_key,
        request,
    )
    .await
}

pub async fn start_passkey_mfa_sign_in(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    request: &StartPasskeyMfaSignInRequest,
) -> AuthResult<StartPasskeyMfaSignInResponse> {
    post_json(
        client,
        endpoint,
        "accounts/mfaSignIn:start",
        api_key,
        request,
    )
    .await
}

pub async fn finalize_phone_mfa_sign_in(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    request: &FinalizePhoneMfaSignInRequest,
) -> AuthResult<FinalizeMfaSignInResponse> {
    post_json(
        client,
        endpoint,
        "accounts/mfaSignIn:finalize",
        api_key,
        request,
    )
    .await
}

pub async fn finalize_totp_mfa_sign_in(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    request: &FinalizeTotpMfaSignInRequest,
) -> AuthResult<FinalizeTotpMfaSignInResponse> {
    post_json(
        client,
        endpoint,
        "accounts/mfaSignIn:finalize",
        api_key,
        request,
    )
    .await
}

pub async fn finalize_passkey_mfa_sign_in(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    request: &FinalizePasskeyMfaSignInRequest,
) -> AuthResult<FinalizeMfaSignInResponse> {
    post_json(
        client,
        endpoint,
        "accounts/mfaSignIn:finalize",
        api_key,
        request,
    )
    .await
}

async fn post_json<TRequest, TResponse>(
    client: &Client,
    endpoint: &str,
    path: &str,
    api_key: &str,
    request: &TRequest,
) -> AuthResult<TResponse>
where
    TRequest: Serialize,
    TResponse: for<'de> Deserialize<'de>,
{
    let url = endpoint_url(endpoint, path, api_key);
    let response = client
        .post(url)
        .json(request)
        .send()
        .await
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        response
            .json::<TResponse>()
            .await
            .map_err(|err| AuthError::InvalidCredential(err.to_string()))
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if let Ok(parsed) = serde_json::from_str::<ErrorResponse>(&body) {
            if let Some(error) = parsed.error {
                if let Some(message) = error.message {
                    if let Some(mapped) = map_mfa_error_code(&message) {
                        return Err(mapped);
                    }
                    return Err(AuthError::InvalidCredential(message));
                }
            }
        }
        Err(AuthError::InvalidCredential(format!(
            "MFA request failed ({status}): {body}"
        )))
    }
}
