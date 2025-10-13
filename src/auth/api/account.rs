use reqwest::blocking::{Client, Response};
use serde::{Deserialize, Serialize};

use crate::auth::error::{AuthError, AuthResult};
use crate::auth::model::{SignInWithPasswordRequest, SignInWithPasswordResponse};

#[derive(Debug, Serialize)]
struct SendOobCodeRequest<'a> {
    #[serde(rename = "requestType")]
    request_type: &'a str,
    #[serde(rename = "email", skip_serializing_if = "Option::is_none")]
    email: Option<&'a str>,
    #[serde(rename = "idToken", skip_serializing_if = "Option::is_none")]
    id_token: Option<&'a str>,
}

pub fn send_password_reset_email(client: &Client, api_key: &str, email: &str) -> AuthResult<()> {
    let request = SendOobCodeRequest {
        request_type: "PASSWORD_RESET",
        email: Some(email),
        id_token: None,
    };
    send_oob_code(client, api_key, &request)
}

pub fn send_email_verification(client: &Client, api_key: &str, id_token: &str) -> AuthResult<()> {
    let request = SendOobCodeRequest {
        request_type: "VERIFY_EMAIL",
        email: None,
        id_token: Some(id_token),
    };
    send_oob_code(client, api_key, &request)
}

fn send_oob_code(
    client: &Client,
    api_key: &str,
    request: &SendOobCodeRequest<'_>,
) -> AuthResult<()> {
    let url =
        format!("https://identitytoolkit.googleapis.com/v1/accounts:sendOobCode?key={api_key}");

    let response = client
        .post(url)
        .json(request)
        .send()
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(map_error(response))
    }
}

#[derive(Debug, Serialize)]
struct ResetPasswordRequest<'a> {
    #[serde(rename = "oobCode")]
    oob_code: &'a str,
    #[serde(rename = "newPassword")]
    new_password: &'a str,
}

pub fn confirm_password_reset(
    client: &Client,
    api_key: &str,
    oob_code: &str,
    new_password: &str,
) -> AuthResult<()> {
    let url =
        format!("https://identitytoolkit.googleapis.com/v1/accounts:resetPassword?key={api_key}");
    let request = ResetPasswordRequest {
        oob_code,
        new_password,
    };

    let response = client
        .post(url)
        .json(&request)
        .send()
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(map_error(response))
    }
}

#[derive(Debug)]
pub enum UpdateString {
    Set(String),
    Clear,
}

#[derive(Debug)]
pub struct UpdateAccountRequest {
    pub id_token: String,
    pub email: Option<String>,
    pub password: Option<String>,
    pub display_name: Option<UpdateString>,
    pub photo_url: Option<UpdateString>,
}

impl UpdateAccountRequest {
    pub fn new(id_token: impl Into<String>) -> Self {
        Self {
            id_token: id_token.into(),
            email: None,
            password: None,
            display_name: None,
            photo_url: None,
        }
    }
}

#[derive(Debug, Serialize)]
struct UpdateAccountRequestBody<'a> {
    #[serde(rename = "idToken")]
    id_token: &'a str,
    #[serde(rename = "email", skip_serializing_if = "Option::is_none")]
    email: Option<&'a str>,
    #[serde(rename = "password", skip_serializing_if = "Option::is_none")]
    password: Option<&'a str>,
    #[serde(rename = "displayName", skip_serializing_if = "Option::is_none")]
    display_name: Option<&'a str>,
    #[serde(rename = "photoUrl", skip_serializing_if = "Option::is_none")]
    photo_url: Option<&'a str>,
    #[serde(rename = "deleteAttribute", skip_serializing_if = "Vec::is_empty")]
    delete_attribute: Vec<&'static str>,
    #[serde(rename = "returnSecureToken")]
    return_secure_token: bool,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ProviderUserInfo {
    #[serde(rename = "providerId")]
    pub provider_id: Option<String>,
    #[serde(rename = "federatedId")]
    pub federated_id: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(rename = "photoUrl")]
    pub photo_url: Option<String>,
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

pub fn update_account(
    client: &Client,
    api_key: &str,
    params: &UpdateAccountRequest,
) -> AuthResult<UpdateAccountResponse> {
    let mut delete_attribute = Vec::new();
    let mut display_name = None;
    if let Some(update) = &params.display_name {
        match update {
            UpdateString::Set(value) => display_name = Some(value.as_str()),
            UpdateString::Clear => delete_attribute.push("DISPLAY_NAME"),
        }
    }

    let mut photo_url = None;
    if let Some(update) = &params.photo_url {
        match update {
            UpdateString::Set(value) => photo_url = Some(value.as_str()),
            UpdateString::Clear => delete_attribute.push("PHOTO_URL"),
        }
    }

    let request = UpdateAccountRequestBody {
        id_token: &params.id_token,
        email: params.email.as_deref(),
        password: params.password.as_deref(),
        display_name,
        photo_url,
        delete_attribute,
        return_secure_token: true,
    };

    let url = format!("https://identitytoolkit.googleapis.com/v1/accounts:update?key={api_key}");

    let response = client
        .post(url)
        .json(&request)
        .send()
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        response
            .json::<UpdateAccountResponse>()
            .map_err(|err| AuthError::InvalidCredential(err.to_string()))
    } else {
        Err(map_error(response))
    }
}

pub fn verify_password(
    client: &Client,
    api_key: &str,
    request: &SignInWithPasswordRequest,
) -> AuthResult<SignInWithPasswordResponse> {
    let url = format!(
        "https://identitytoolkit.googleapis.com/v1/accounts:signInWithPassword?key={api_key}"
    );
    let response = client
        .post(url)
        .json(request)
        .send()
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        response
            .json::<SignInWithPasswordResponse>()
            .map_err(|err| AuthError::InvalidCredential(err.to_string()))
    } else {
        Err(map_error(response))
    }
}

#[derive(Debug, Serialize)]
struct DeleteAccountRequest<'a> {
    #[serde(rename = "idToken")]
    id_token: &'a str,
}

pub fn delete_account(client: &Client, api_key: &str, id_token: &str) -> AuthResult<()> {
    let url = format!("https://identitytoolkit.googleapis.com/v1/accounts:delete?key={api_key}");
    let request = DeleteAccountRequest { id_token };

    let response = client
        .post(url)
        .json(&request)
        .send()
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(map_error(response))
    }
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: Option<ErrorBody>,
}

fn map_error(response: Response) -> AuthError {
    let status = response.status();
    let body = response.text().unwrap_or_default();

    if let Ok(parsed) = serde_json::from_str::<ErrorResponse>(&body) {
        if let Some(error) = parsed.error {
            if let Some(message) = error.message {
                return AuthError::InvalidCredential(message);
            }
        }
    }

    AuthError::InvalidCredential(format!("Request failed with status {status}: {body}",))
}
