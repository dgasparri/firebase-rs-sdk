use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

use crate::auth::error::{map_mfa_error_code, AuthError, AuthResult};

pub(crate) const DEFAULT_SECURE_TOKEN_ENDPOINT: &str =
    "https://securetoken.googleapis.com/v1/token";

#[derive(Debug, Serialize)]
struct RefreshTokenRequest {
    grant_type: &'static str,
    refresh_token: String,
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

pub async fn refresh_id_token(
    client: &Client,
    api_key: &str,
    refresh_token: &str,
) -> AuthResult<RefreshTokenResponse> {
    refresh_id_token_with_endpoint(
        client,
        DEFAULT_SECURE_TOKEN_ENDPOINT,
        api_key,
        refresh_token,
    )
    .await
}

pub async fn refresh_id_token_with_endpoint(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    refresh_token: &str,
) -> AuthResult<RefreshTokenResponse> {
    let url = format!("{endpoint}?key={api_key}");
    let request = RefreshTokenRequest {
        grant_type: "refresh_token",
        refresh_token: refresh_token.to_owned(),
    };

    let response = client
        .post(url)
        .form(&request)
        .send()
        .await
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        response
            .json::<RefreshTokenResponse>()
            .await
            .map_err(|err| AuthError::Network(err.to_string()))
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| "{}".to_string());
        Err(map_refresh_error(status, &body))
    }
}

fn map_refresh_error(status: StatusCode, body: &str) -> AuthError {
    if let Ok(parsed) = serde_json::from_str::<ErrorResponse>(body) {
        if let Some(error) = parsed.error {
            if let Some(message) = error.message {
                if let Some(mapped) = map_mfa_error_code(&message) {
                    return mapped;
                }
                return AuthError::InvalidCredential(message);
            }
        }
    }

    AuthError::Network(format!("Token refresh failed with status {status}"))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::auth::error::MultiFactorAuthErrorCode;
    use crate::test_support::start_mock_server;
    use httpmock::prelude::*;
    use serde_json::json;

    fn make_client() -> Client {
        Client::new()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn refresh_id_token_succeeds_with_custom_endpoint() {
        let server = start_mock_server();
        let client = make_client();

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/token")
                .query_param("key", "test-key")
                .header("content-type", "application/x-www-form-urlencoded")
                .body_contains("grant_type=refresh_token")
                .body_contains("refresh_token=test-refresh");
            then.status(200).json_body(json!({
                "access_token": "access",
                "refresh_token": "new-refresh",
                "id_token": "id",
                "expires_in": "3600",
                "user_id": "uid"
            }));
        });

        let response = refresh_id_token_with_endpoint(
            &client,
            &server.url("/token"),
            "test-key",
            "test-refresh",
        )
        .await
        .expect("refresh should succeed");

        mock.assert();
        assert_eq!(response.access_token, "access");
        assert_eq!(response.refresh_token, "new-refresh");
        assert_eq!(response.id_token, "id");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn refresh_id_token_maps_error_message() {
        let server = start_mock_server();
        let client = make_client();

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/token")
                .query_param("key", "test-key");
            then.status(400)
                .body("{\"error\":{\"message\":\"TOKEN_EXPIRED\"}}");
        });

        let result = refresh_id_token_with_endpoint(
            &client,
            &server.url("/token"),
            "test-key",
            "test-refresh",
        )
        .await;

        mock.assert();
        assert!(matches!(
            result,
            Err(AuthError::InvalidCredential(message)) if message == "TOKEN_EXPIRED"
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn refresh_id_token_maps_mfa_errors() {
        let server = start_mock_server();
        let client = make_client();

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/token")
                .query_param("key", "test-key");
            then.status(400)
                .body("{\"error\":{\"message\":\"MISSING_MFA_PENDING_CREDENTIAL\"}}");
        });

        let result = refresh_id_token_with_endpoint(
            &client,
            &server.url("/token"),
            "test-key",
            "test-refresh",
        )
        .await;

        mock.assert();
        match result {
            Err(AuthError::MultiFactor(err)) => {
                assert_eq!(err.code(), MultiFactorAuthErrorCode::MissingSession);
            }
            other => panic!("expected MFA error, got {other:?}"),
        }
    }
}
