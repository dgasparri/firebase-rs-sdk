use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

use crate::auth::error::{AuthError, AuthResult};
use crate::auth::model::{
    GetAccountInfoResponse, ProviderUserInfo, SignInWithPasswordRequest, SignInWithPasswordResponse,
};

fn identity_toolkit_url(base: &str, path: &str, api_key: &str) -> String {
    format!("{}/{}?key={}", base.trim_end_matches('/'), path, api_key)
}

#[derive(Debug, Serialize)]
struct SendOobCodeRequest {
    #[serde(rename = "requestType")]
    request_type: &'static str,
    #[serde(rename = "email", skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(rename = "idToken", skip_serializing_if = "Option::is_none")]
    id_token: Option<String>,
}

#[derive(Debug, Serialize)]
struct ResetPasswordRequest {
    #[serde(rename = "oobCode")]
    oob_code: String,
    #[serde(rename = "newPassword")]
    new_password: String,
}

#[derive(Debug, Clone)]
pub enum UpdateString {
    Set(String),
    Clear,
}

#[derive(Debug, Clone)]
pub struct UpdateAccountRequest {
    pub id_token: String,
    pub email: Option<String>,
    pub password: Option<String>,
    pub display_name: Option<UpdateString>,
    pub photo_url: Option<UpdateString>,
    pub delete_providers: Vec<String>,
}

impl UpdateAccountRequest {
    pub fn new(id_token: impl Into<String>) -> Self {
        Self {
            id_token: id_token.into(),
            email: None,
            password: None,
            display_name: None,
            photo_url: None,
            delete_providers: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct UpdateAccountRequestBody {
    #[serde(rename = "idToken")]
    id_token: String,
    #[serde(rename = "email", skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(rename = "password", skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    #[serde(rename = "displayName", skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    #[serde(rename = "photoUrl", skip_serializing_if = "Option::is_none")]
    photo_url: Option<String>,
    #[serde(rename = "deleteAttribute", skip_serializing_if = "Vec::is_empty")]
    delete_attribute: Vec<&'static str>,
    #[serde(rename = "deleteProvider", skip_serializing_if = "Vec::is_empty")]
    delete_provider: Vec<String>,
    #[serde(rename = "returnSecureToken")]
    return_secure_token: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAccountResponse {
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
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(rename = "photoUrl")]
    pub photo_url: Option<String>,
    #[serde(rename = "providerUserInfo")]
    pub provider_user_info: Option<Vec<ProviderUserInfo>>,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: Option<ErrorBody>,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    message: Option<String>,
}

pub async fn send_password_reset_email(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    email: &str,
) -> AuthResult<()> {
    send_oob_code_async(
        client.clone(),
        endpoint.to_owned(),
        api_key.to_owned(),
        SendOobCodeRequest {
            request_type: "PASSWORD_RESET",
            email: Some(email.to_owned()),
            id_token: None,
        },
    )
    .await
}

pub async fn send_email_verification(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    id_token: &str,
) -> AuthResult<()> {
    send_oob_code_async(
        client.clone(),
        endpoint.to_owned(),
        api_key.to_owned(),
        SendOobCodeRequest {
            request_type: "VERIFY_EMAIL",
            email: None,
            id_token: Some(id_token.to_owned()),
        },
    )
    .await
}

async fn send_oob_code_async(
    client: Client,
    endpoint: String,
    api_key: String,
    request: SendOobCodeRequest,
) -> AuthResult<()> {
    let url = identity_toolkit_url(&endpoint, "accounts:sendOobCode", &api_key);
    let response = client
        .post(url)
        .json(&request)
        .send()
        .await
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        Ok(())
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| String::new());
        Err(map_error(status, body))
    }
}

pub async fn confirm_password_reset(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    oob_code: &str,
    new_password: &str,
) -> AuthResult<()> {
    confirm_password_reset_async(
        client.clone(),
        endpoint.to_owned(),
        api_key.to_owned(),
        ResetPasswordRequest {
            oob_code: oob_code.to_owned(),
            new_password: new_password.to_owned(),
        },
    )
    .await
}

async fn confirm_password_reset_async(
    client: Client,
    endpoint: String,
    api_key: String,
    request: ResetPasswordRequest,
) -> AuthResult<()> {
    let url = identity_toolkit_url(&endpoint, "accounts:resetPassword", &api_key);
    let response = client
        .post(url)
        .json(&request)
        .send()
        .await
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        Ok(())
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| String::new());
        Err(map_error(status, body))
    }
}

pub async fn update_account(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    params: &UpdateAccountRequest,
) -> AuthResult<UpdateAccountResponse> {
    update_account_async(
        client.clone(),
        endpoint.to_owned(),
        api_key.to_owned(),
        params.clone(),
    )
    .await
}

async fn update_account_async(
    client: Client,
    endpoint: String,
    api_key: String,
    params: UpdateAccountRequest,
) -> AuthResult<UpdateAccountResponse> {
    let UpdateAccountRequest {
        id_token,
        email,
        password,
        display_name,
        photo_url,
        delete_providers,
    } = params;

    let mut delete_attribute = Vec::new();
    let display_name = match display_name {
        Some(UpdateString::Set(value)) => Some(value),
        Some(UpdateString::Clear) => {
            delete_attribute.push("DISPLAY_NAME");
            None
        }
        None => None,
    };

    let photo_url = match photo_url {
        Some(UpdateString::Set(value)) => Some(value),
        Some(UpdateString::Clear) => {
            delete_attribute.push("PHOTO_URL");
            None
        }
        None => None,
    };

    let request = UpdateAccountRequestBody {
        id_token,
        email,
        password,
        display_name,
        photo_url,
        delete_attribute,
        delete_provider: delete_providers,
        return_secure_token: true,
    };

    let url = identity_toolkit_url(&endpoint, "accounts:update", &api_key);
    let response = client
        .post(url)
        .json(&request)
        .send()
        .await
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        response
            .json::<UpdateAccountResponse>()
            .await
            .map_err(|err| AuthError::InvalidCredential(err.to_string()))
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| String::new());
        Err(map_error(status, body))
    }
}

pub async fn verify_password(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    request: &SignInWithPasswordRequest,
) -> AuthResult<SignInWithPasswordResponse> {
    verify_password_async(
        client.clone(),
        endpoint.to_owned(),
        api_key.to_owned(),
        request.clone(),
    )
    .await
}

async fn verify_password_async(
    client: Client,
    endpoint: String,
    api_key: String,
    request: SignInWithPasswordRequest,
) -> AuthResult<SignInWithPasswordResponse> {
    let url = identity_toolkit_url(&endpoint, "accounts:signInWithPassword", &api_key);
    let response = client
        .post(url)
        .json(&request)
        .send()
        .await
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        response
            .json::<SignInWithPasswordResponse>()
            .await
            .map_err(|err| AuthError::InvalidCredential(err.to_string()))
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| String::new());
        Err(map_error(status, body))
    }
}

pub async fn delete_account(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    id_token: &str,
) -> AuthResult<()> {
    delete_account_async(
        client.clone(),
        endpoint.to_owned(),
        api_key.to_owned(),
        id_token.to_owned(),
    )
    .await
}

async fn delete_account_async(
    client: Client,
    endpoint: String,
    api_key: String,
    id_token: String,
) -> AuthResult<()> {
    let url = identity_toolkit_url(&endpoint, "accounts:delete", &api_key);
    let request = DeleteAccountRequest { id_token };

    let response = client
        .post(url)
        .json(&request)
        .send()
        .await
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        Ok(())
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| String::new());
        Err(map_error(status, body))
    }
}

#[derive(Debug, Serialize)]
struct DeleteAccountRequest {
    #[serde(rename = "idToken")]
    id_token: String,
}

#[derive(Debug, Serialize)]
struct GetAccountInfoRequest {
    #[serde(rename = "idToken")]
    id_token: String,
}

pub async fn get_account_info(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    id_token: &str,
) -> AuthResult<GetAccountInfoResponse> {
    get_account_info_async(
        client.clone(),
        endpoint.to_owned(),
        api_key.to_owned(),
        id_token.to_owned(),
    )
    .await
}

async fn get_account_info_async(
    client: Client,
    endpoint: String,
    api_key: String,
    id_token: String,
) -> AuthResult<GetAccountInfoResponse> {
    let url = identity_toolkit_url(&endpoint, "accounts:lookup", &api_key);
    let request = GetAccountInfoRequest { id_token };

    let response = client
        .post(url)
        .json(&request)
        .send()
        .await
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        response
            .json::<GetAccountInfoResponse>()
            .await
            .map_err(|err| AuthError::InvalidCredential(err.to_string()))
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| String::new());
        Err(map_error(status, body))
    }
}

fn map_error(status: StatusCode, body: String) -> AuthError {
    if let Ok(parsed) = serde_json::from_str::<ErrorResponse>(&body) {
        if let Some(error) = parsed.error {
            if let Some(message) = error.message {
                return AuthError::InvalidCredential(message);
            }
        }
    }

    AuthError::InvalidCredential(format!("Request failed with status {status}: {body}"))
}
