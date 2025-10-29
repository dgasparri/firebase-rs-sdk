use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::auth::error::{map_mfa_error_code, AuthError, AuthResult};
use crate::auth::model::MfaEnrollmentInfo;

fn endpoint_url(base: &str, path: &str, api_key: &str) -> String {
    format!("{}/{}?key={}", base.trim_end_matches('/'), path, api_key)
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: Option<ErrorBody>,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    message: Option<String>,
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct SendPhoneVerificationCodeRequest {
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
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SendPhoneVerificationCodeResponse {
    #[serde(rename = "sessionInfo")]
    pub session_info: String,
}

pub async fn send_phone_verification_code(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    request: &SendPhoneVerificationCodeRequest,
) -> AuthResult<SendPhoneVerificationCodeResponse> {
    let url = endpoint_url(endpoint, "accounts:sendVerificationCode", api_key);
    let response = client
        .post(url)
        .json(request)
        .send()
        .await
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        response
            .json::<SendPhoneVerificationCodeResponse>()
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
            "sendVerificationCode failed ({status}): {body}"
        )))
    }
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct SignInWithPhoneNumberRequest {
    #[serde(rename = "temporaryProof", skip_serializing_if = "Option::is_none")]
    pub temporary_proof: Option<String>,
    #[serde(rename = "phoneNumber", skip_serializing_if = "Option::is_none")]
    pub phone_number: Option<String>,
    #[serde(rename = "sessionInfo", skip_serializing_if = "Option::is_none")]
    pub session_info: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(rename = "idToken", skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<&'static str>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PhoneSignInResponse {
    #[serde(rename = "idToken")]
    pub id_token: Option<String>,
    #[serde(rename = "refreshToken")]
    pub refresh_token: Option<String>,
    #[serde(rename = "expiresIn")]
    pub expires_in: Option<String>,
    #[serde(rename = "localId")]
    pub local_id: Option<String>,
    #[serde(rename = "isNewUser")]
    pub is_new_user: Option<bool>,
    #[serde(rename = "phoneNumber")]
    pub phone_number: Option<String>,
    #[serde(rename = "temporaryProof")]
    pub temporary_proof: Option<String>,
    #[serde(rename = "mfaPendingCredential")]
    pub mfa_pending_credential: Option<String>,
    #[serde(rename = "mfaInfo")]
    pub mfa_info: Option<Vec<MfaEnrollmentInfo>>,
}

pub async fn sign_in_with_phone_number(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    request: &SignInWithPhoneNumberRequest,
) -> AuthResult<PhoneSignInResponse> {
    execute_phone_request(client, endpoint, api_key, request).await
}

pub async fn link_with_phone_number(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    request: &SignInWithPhoneNumberRequest,
) -> AuthResult<PhoneSignInResponse> {
    let response = execute_phone_request(client, endpoint, api_key, request).await?;
    if response.temporary_proof.is_some() {
        return Err(AuthError::InvalidCredential(
            "Phone number linking requires confirmation".into(),
        ));
    }
    Ok(response)
}

pub async fn verify_phone_number_for_existing(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    request: &SignInWithPhoneNumberRequest,
) -> AuthResult<PhoneSignInResponse> {
    let mut payload = request.clone();
    payload.operation = Some("REAUTH");
    execute_phone_request(client, endpoint, api_key, &payload).await
}

async fn execute_phone_request(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    request: &SignInWithPhoneNumberRequest,
) -> AuthResult<PhoneSignInResponse> {
    let url = endpoint_url(endpoint, "accounts:signInWithPhoneNumber", api_key);
    let response = client
        .post(url)
        .json(request)
        .send()
        .await
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        response
            .json::<PhoneSignInResponse>()
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
            "signInWithPhoneNumber failed ({status}): {body}"
        )))
    }
}
