use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::auth::error::{AuthError, AuthResult};
use crate::auth::model::MfaEnrollmentInfo;

/// Fields returned by the `signInWithIdp` Firebase Auth REST endpoint.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct SignInWithIdpResponse {
    #[serde(rename = "idToken")]
    pub id_token: Option<String>,
    #[serde(rename = "refreshToken")]
    pub refresh_token: Option<String>,
    #[serde(rename = "expiresIn")]
    pub expires_in: Option<String>,
    #[serde(rename = "localId")]
    pub local_id: Option<String>,
    #[serde(rename = "email")]
    pub email: Option<String>,
    #[serde(rename = "isNewUser")]
    pub is_new_user: Option<bool>,
    #[serde(rename = "oauthAccessToken")]
    pub oauth_access_token: Option<String>,
    #[serde(rename = "oauthIdToken")]
    pub oauth_id_token: Option<String>,
    #[serde(rename = "providerId")]
    pub provider_id: Option<String>,
    #[serde(rename = "mfaPendingCredential")]
    pub mfa_pending_credential: Option<String>,
    #[serde(rename = "mfaInfo")]
    pub mfa_info: Option<Vec<MfaEnrollmentInfo>>,
}

#[derive(Debug, Serialize, Clone)]
pub struct SignInWithIdpRequest {
    #[serde(rename = "postBody")]
    pub post_body: String,
    #[serde(rename = "requestUri")]
    pub request_uri: String,
    #[serde(rename = "returnIdpCredential")]
    pub return_idp_credential: bool,
    #[serde(rename = "returnSecureToken")]
    pub return_secure_token: bool,
    #[serde(rename = "idToken", skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
}

/// Signs a user in with an identity provider using the `signInWithIdp` REST endpoint.
pub async fn sign_in_with_idp(
    client: &Client,
    api_key: &str,
    request: &SignInWithIdpRequest,
) -> AuthResult<SignInWithIdpResponse> {
    sign_in_with_idp_async(client.clone(), api_key.to_owned(), request.clone()).await
}

async fn sign_in_with_idp_async(
    client: Client,
    api_key: String,
    request: SignInWithIdpRequest,
) -> AuthResult<SignInWithIdpResponse> {
    let url = format!("https://identitytoolkit.googleapis.com/v1/accounts:signInWithIdp?key={api_key}");

    let response = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .map_err(|err| AuthError::Network(err.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AuthError::InvalidCredential(format!("signInWithIdp failed ({status}): {body}")));
    }

    response
        .json::<SignInWithIdpResponse>()
        .await
        .map_err(|err| AuthError::InvalidCredential(err.to_string()))
}
