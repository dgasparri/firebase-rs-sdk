#[cfg(not(target_arch = "wasm32"))]
mod native_tests {
    use crate::messaging::error::token_subscribe_no_token;
    use crate::messaging::fcm_rest::{
        FcmClient, FcmRegistrationRequest, FcmSubscription, FcmUpdateRequest,
    };
    use httpmock::prelude::*;
    use serde_json::json;

    fn client_with_server(server: &MockServer) -> FcmClient {
        std::env::set_var("FIREBASE_MESSAGING_FCM_ENDPOINT", server.base_url());
        FcmClient::new().expect("client")
    }

    fn subscription<'a>() -> FcmSubscription<'a> {
        FcmSubscription {
            endpoint: "https://example.org",
            auth: "auth-value",
            p256dh: "p256dh-value",
            application_pub_key: Some("vapid-key"),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn register_token_success() {
        let server = MockServer::start();
        let client = client_with_server(&server);
        std::env::remove_var("FIREBASE_MESSAGING_FCM_ENDPOINT");

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/projects/project-id/registrations")
                .header("x-goog-api-key", "api-key")
                .header("x-goog-firebase-installations-auth", "FIS auth-token");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({"token": "fcm-token"}));
        });

        let request = FcmRegistrationRequest {
            project_id: "project-id",
            api_key: "api-key",
            installation_auth_token: "auth-token",
            subscription: subscription(),
        };

        let token = client.register_token(&request).await.unwrap();
        assert_eq!(token, "fcm-token");
        mock.assert();
        std::env::remove_var("FIREBASE_MESSAGING_FCM_ENDPOINT");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn register_token_no_token_errors() {
        let server = MockServer::start();
        let client = client_with_server(&server);
        std::env::remove_var("FIREBASE_MESSAGING_FCM_ENDPOINT");

        server.mock(|when, then| {
            when.method(POST).path("/projects/project-id/registrations");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({}));
        });

        let request = FcmRegistrationRequest {
            project_id: "project-id",
            api_key: "api-key",
            installation_auth_token: "auth-token",
            subscription: subscription(),
        };

        let err = client.register_token(&request).await.unwrap_err();
        assert_eq!(err.code_str(), token_subscribe_no_token().code_str());
        std::env::remove_var("FIREBASE_MESSAGING_FCM_ENDPOINT");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn update_token_success() {
        let server = MockServer::start();
        let client = client_with_server(&server);
        std::env::remove_var("FIREBASE_MESSAGING_FCM_ENDPOINT");

        let mock = server.mock(|when, then| {
            when.method("PATCH")
                .path("/projects/project-id/registrations/token-123")
                .header("x-goog-api-key", "api-key")
                .header("x-goog-firebase-installations-auth", "FIS auth-token");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({"token": "updated-token"}));
        });

        let request = FcmUpdateRequest {
            registration_token: "token-123",
            registration: FcmRegistrationRequest {
                project_id: "project-id",
                api_key: "api-key",
                installation_auth_token: "auth-token",
                subscription: subscription(),
            },
        };

        let token = client.update_token(&request).await.unwrap();
        assert_eq!(token, "updated-token");
        mock.assert();
        std::env::remove_var("FIREBASE_MESSAGING_FCM_ENDPOINT");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn delete_token_success() {
        let server = MockServer::start();
        let client = client_with_server(&server);
        std::env::remove_var("FIREBASE_MESSAGING_FCM_ENDPOINT");

        let mock = server.mock(|when, then| {
            when.method(DELETE)
                .path("/projects/project-id/registrations/token-123")
                .header("x-goog-api-key", "api-key")
                .header("x-goog-firebase-installations-auth", "FIS auth-token");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({}));
        });

        client
            .delete_token("project-id", "api-key", "auth-token", "token-123")
            .await
            .unwrap();
        mock.assert();
        std::env::remove_var("FIREBASE_MESSAGING_FCM_ENDPOINT");
    }
}
