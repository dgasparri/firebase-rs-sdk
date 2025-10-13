use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::auth::error::{AuthError, AuthResult};

#[derive(Debug, Serialize)]
struct RefreshTokenRequest<'a> {
    grant_type: &'static str,
    refresh_token: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct RefreshTokenResponse {
    #[serde(rename = "access_token")]
    pub access_token: String,
    #[serde(rename = "refresh_token")]
    pub refresh_token: String,
    #[serde(rename = "id_token")]
    pub id_token: String,
    #[serde(rename = "expires_in")]
    pub expires_in: String,
    #[serde(rename = "user_id")]
    pub user_id: String,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: Option<ErrorBody>,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    message: Option<String>,
}

pub fn refresh_id_token(
    client: &Client,
    api_key: &str,
    refresh_token: &str,
) -> AuthResult<RefreshTokenResponse> {
    let url = format!(
        "https://securetoken.googleapis.com/v1/token?key={}",
        api_key
    );
    let request = RefreshTokenRequest {
        grant_type: "refresh_token",
        refresh_token,
    };

    let response = client
        .post(url)
        .form(&request)
        .send()
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        response
            .json()
            .map_err(|err| AuthError::Network(err.to_string()))
    } else {
        let status = response.status();
        let body = response.text().unwrap_or_else(|_| "{}".to_string());
        Err(map_refresh_error(status, &body))
    }
}

fn map_refresh_error(status: StatusCode, body: &str) -> AuthError {
    if let Ok(parsed) = serde_json::from_str::<ErrorResponse>(body) {
        if let Some(error) = parsed.error {
            if let Some(message) = error.message {
                return AuthError::InvalidCredential(message);
            }
        }
    }

    AuthError::Network(format!("Token refresh failed with status {}", status))
}
