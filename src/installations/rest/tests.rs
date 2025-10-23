use super::*;
use crate::installations::error::InstallationsErrorCode;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use crate::installations::config::AppConfig;
    use crate::installations::error::InstallationsErrorCode;
    use httpmock::prelude::*;
    use serde_json::json;
    use std::panic::{self, AssertUnwindSafe};

    fn test_config() -> AppConfig {
        AppConfig {
            app_name: "test".into(),
            api_key: "key".into(),
            project_id: "project".into(),
            app_id: "app".into(),
        }
    }

    fn try_start_server() -> Option<MockServer> {
        panic::catch_unwind(AssertUnwindSafe(|| MockServer::start())).ok()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn register_installation_success() {
        let Some(server) = try_start_server() else {
            eprintln!("Skipping register_installation_success: unable to start mock server");
            return;
        };
        let _mock = server.mock(|when, then| {
            when.method(POST)
                .path("/projects/project/installations")
                .header("x-goog-api-key", "key");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "refreshToken": "refresh",
                    "authToken": {
                        "token": "token",
                        "expiresIn": "3600s"
                    },
                    "fid": "fid"
                }));
        });

        let client = RestClient::with_base_url(&server.base_url()).unwrap();
        let result = client
            .register_installation(&test_config(), "local-fid")
            .await
            .unwrap();

        assert_eq!(result.fid, "fid");
        assert_eq!(result.refresh_token, "refresh");
        assert_eq!(result.auth_token.token, "token");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn generate_auth_token_success() {
        let Some(server) = try_start_server() else {
            eprintln!("Skipping generate_auth_token_success: unable to start mock server");
            return;
        };
        let _mock = server.mock(|when, then| {
            when.method(POST)
                .path("/projects/project/installations/fid/authTokens:generate")
                .header("x-goog-api-key", "key")
                .header(
                    "authorization",
                    format!("{} {}", INTERNAL_AUTH_VERSION, "refresh"),
                );
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "token": "token",
                    "expiresIn": "7200s"
                }));
        });

        let client = RestClient::with_base_url(&server.base_url()).unwrap();
        let token = client
            .generate_auth_token(&test_config(), "fid", "refresh")
            .await
            .unwrap();

        assert_eq!(token.token, "token");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn delete_installation_success() {
        let Some(server) = try_start_server() else {
            eprintln!("Skipping delete_installation_success: unable to start mock server");
            return;
        };

        let _mock = server.mock(|when, then| {
            when.method(DELETE)
                .path("/projects/project/installations/fid")
                .header("x-goog-api-key", "key")
                .header(
                    "authorization",
                    format!("{} {}", INTERNAL_AUTH_VERSION, "refresh"),
                );
            then.status(200);
        });

        let client = RestClient::with_base_url(&server.base_url()).unwrap();
        client
            .delete_installation(&test_config(), "fid", "refresh")
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn server_error_retries_once() {
        let Some(server) = try_start_server() else {
            eprintln!("Skipping server_error_retries_once: unable to start mock server");
            return;
        };

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/projects/project/installations")
                .header("x-goog-api-key", "key");
            then.status(500);
        });

        let client = RestClient::with_base_url(&server.base_url()).unwrap();
        let err = client
            .register_installation(&test_config(), "fid")
            .await
            .unwrap_err();

        assert_eq!(mock.hits(), 2);
        assert!(matches!(err.code, InstallationsErrorCode::RequestFailed));
    }
}

#[test]
fn parse_expires_in_rejects_invalid_format() {
    let err = super::parse_expires_in("1000ms").unwrap_err();
    assert!(matches!(err.code, InstallationsErrorCode::InvalidArgument));
}
