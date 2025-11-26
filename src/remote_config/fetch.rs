//! Remote Config fetch client abstractions.
//!
//! This mirrors the TypeScript `RemoteConfigFetchClient` interface in
//! `packages/remote-config/src/client/remote_config_fetch_client.ts`, providing a pluggable
//! transport layer for retrieving templates from the backend.

use std::collections::HashMap;
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

use crate::installations::Installations;
use crate::installations::InstallationsResult;
use crate::remote_config::error::{internal_error, RemoteConfigResult};
use serde::Deserialize;
use serde_json::{json, Map as JsonMap, Value as JsonValue};

#[cfg(not(target_arch = "wasm32"))]
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, IF_NONE_MATCH};
#[cfg(not(target_arch = "wasm32"))]
use reqwest::{Client, StatusCode};
#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
use reqwest::{Client, StatusCode};

/// Parameters describing a fetch attempt.
#[derive(Clone, Debug, PartialEq)]
pub struct FetchRequest {
    /// Maximum allowed age for cached results before a network call should be forced.
    pub cache_max_age_millis: u64,
    /// Timeout budget for the request.
    pub timeout_millis: u64,
    /// Optional entity tag to include via `If-None-Match`.
    pub e_tag: Option<String>,
    /// Optional custom signals payload forwarded to the backend.
    pub custom_signals: Option<HashMap<String, JsonValue>>,
}

/// Minimal representation of the Remote Config REST response.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FetchResponse {
    pub status: u16,
    pub etag: Option<String>,
    pub config: Option<HashMap<String, String>>,
    pub template_version: Option<u64>,
}

/// Abstraction over the network layer used to retrieve Remote Config templates.
#[cfg_attr(
    all(feature = "wasm-web", target_arch = "wasm32"),
    async_trait::async_trait(?Send)
)]
#[cfg_attr(not(all(feature = "wasm-web", target_arch = "wasm32")), async_trait::async_trait)]
pub trait RemoteConfigFetchClient: Send + Sync {
    async fn fetch(&self, request: FetchRequest) -> RemoteConfigResult<FetchResponse>;
}

/// Default stub fetch client: returns an empty template with a 200 status.
#[derive(Default)]
pub struct NoopFetchClient;

#[cfg_attr(
    all(feature = "wasm-web", target_arch = "wasm32"),
    async_trait::async_trait(?Send)
)]
#[cfg_attr(not(all(feature = "wasm-web", target_arch = "wasm32")), async_trait::async_trait)]
impl RemoteConfigFetchClient for NoopFetchClient {
    async fn fetch(&self, request: FetchRequest) -> RemoteConfigResult<FetchResponse> {
        let _ = request;
        Ok(FetchResponse {
            status: 200,
            etag: None,
            config: Some(HashMap::new()),
            template_version: None,
        })
    }
}

fn map_installations_error<T>(result: InstallationsResult<T>) -> RemoteConfigResult<T> {
    result.map_err(|err| internal_error(err.to_string()))
}

#[cfg_attr(
    all(feature = "wasm-web", target_arch = "wasm32"),
    async_trait::async_trait(?Send)
)]
#[cfg_attr(not(all(feature = "wasm-web", target_arch = "wasm32")), async_trait::async_trait)]
pub trait InstallationsTokenProvider: Send + Sync {
    async fn installation_id(&self) -> InstallationsResult<String>;
    async fn installation_token(&self) -> InstallationsResult<String>;
}

#[cfg_attr(
    all(feature = "wasm-web", target_arch = "wasm32"),
    async_trait::async_trait(?Send)
)]
#[cfg_attr(not(all(feature = "wasm-web", target_arch = "wasm32")), async_trait::async_trait)]
impl InstallationsTokenProvider for Installations {
    async fn installation_id(&self) -> InstallationsResult<String> {
        self.get_id().await
    }

    async fn installation_token(&self) -> InstallationsResult<String> {
        Ok(self.get_token(false).await?.token)
    }
}

#[derive(Deserialize)]
struct RestFetchResponse {
    #[serde(default)]
    entries: Option<HashMap<String, String>>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default, rename = "templateVersion")]
    template_version: Option<u64>,
}

/// Blocking HTTP implementation for the Remote Config REST API.
#[cfg(not(target_arch = "wasm32"))]
pub struct HttpRemoteConfigFetchClient {
    client: Client,
    base_url: String,
    project_id: String,
    namespace: String,
    api_key: String,
    app_id: String,
    sdk_version: String,
    language_code: String,
    installations: Arc<dyn InstallationsTokenProvider>,
}

#[cfg(not(target_arch = "wasm32"))]
impl HttpRemoteConfigFetchClient {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client: Client,
        base_url: impl Into<String>,
        project_id: impl Into<String>,
        namespace: impl Into<String>,
        api_key: impl Into<String>,
        app_id: impl Into<String>,
        sdk_version: impl Into<String>,
        language_code: impl Into<String>,
        installations: Arc<dyn InstallationsTokenProvider>,
    ) -> Self {
        Self {
            client,
            base_url: base_url.into(),
            project_id: project_id.into(),
            namespace: namespace.into(),
            api_key: api_key.into(),
            app_id: app_id.into(),
            sdk_version: sdk_version.into(),
            language_code: language_code.into(),
            installations,
        }
    }

    fn build_headers(&self, e_tag: Option<&str>) -> RemoteConfigResult<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            IF_NONE_MATCH,
            HeaderValue::from_str(e_tag.unwrap_or("*"))
                .map_err(|err| internal_error(format!("invalid ETag: {err}")))?,
        );
        Ok(headers)
    }

    fn request_body(
        &self,
        installation_id: String,
        installation_token: String,
        custom_signals: Option<HashMap<String, JsonValue>>,
    ) -> JsonValue {
        let mut payload = json!({
            "sdk_version": self.sdk_version,
            "app_instance_id": installation_id,
            "app_instance_id_token": installation_token,
            "app_id": self.app_id,
            "language_code": self.language_code,
        });

        if let Some(signals) = custom_signals {
            if let Some(obj) = payload.as_object_mut() {
                let mut map = JsonMap::with_capacity(signals.len());
                for (key, value) in signals {
                    map.insert(key, value);
                }
                obj.insert("custom_signals".to_string(), JsonValue::Object(map));
            }
        }

        payload
    }

    fn build_url(&self) -> String {
        format!(
            "{}/v1/projects/{}/namespaces/{}:fetch?key={}",
            self.base_url, self.project_id, self.namespace, self.api_key
        )
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait::async_trait]
impl RemoteConfigFetchClient for HttpRemoteConfigFetchClient {
    async fn fetch(&self, request: FetchRequest) -> RemoteConfigResult<FetchResponse> {
        let installation_id = map_installations_error(self.installations.installation_id().await)?;
        let installation_token = map_installations_error(self.installations.installation_token().await)?;
        let url = self.build_url();

        let headers = self.build_headers(request.e_tag.as_deref())?;
        let body = self.request_body(installation_id, installation_token, request.custom_signals);

        let mut builder = self.client.post(url).headers(headers).json(&body);

        builder = builder.timeout(Duration::from_millis(request.timeout_millis));

        let response = builder
            .send()
            .await
            .map_err(|err| internal_error(format!("remote config fetch failed: {err}")))?;

        let mut status = response.status();
        let e_tag = response
            .headers()
            .get("ETag")
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string());

        let response_body = if status == StatusCode::OK {
            Some(
                response
                    .json::<RestFetchResponse>()
                    .await
                    .map_err(|err| internal_error(format!("failed to parse Remote Config response: {err}")))?,
            )
        } else if status == StatusCode::NOT_MODIFIED {
            None
        } else {
            return Err(internal_error(format!("fetch returned unexpected status {}", status.as_u16())));
        };

        let mut config = response_body.as_ref().and_then(|body| body.entries.clone());
        let state = response_body.as_ref().and_then(|body| body.state.clone());
        let template_version = response_body.as_ref().and_then(|body| body.template_version);

        match state.as_deref() {
            Some("INSTANCE_STATE_UNSPECIFIED") => status = StatusCode::INTERNAL_SERVER_ERROR,
            Some("NO_CHANGE") => status = StatusCode::NOT_MODIFIED,
            Some("NO_TEMPLATE") | Some("EMPTY_CONFIG") => {
                config = Some(HashMap::new());
            }
            _ => {}
        }

        match status {
            StatusCode::OK | StatusCode::NOT_MODIFIED => Ok(FetchResponse {
                status: status.as_u16(),
                etag: e_tag,
                config,
                template_version,
            }),
            other => Err(internal_error(format!("fetch returned unexpected status {}", other.as_u16()))),
        }
    }
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
pub struct WasmRemoteConfigFetchClient {
    client: Client,
    base_url: String,
    project_id: String,
    namespace: String,
    api_key: String,
    app_id: String,
    sdk_version: String,
    language_code: String,
    installations: Arc<dyn InstallationsTokenProvider>,
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
impl WasmRemoteConfigFetchClient {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client: Client,
        base_url: impl Into<String>,
        project_id: impl Into<String>,
        namespace: impl Into<String>,
        api_key: impl Into<String>,
        app_id: impl Into<String>,
        sdk_version: impl Into<String>,
        language_code: impl Into<String>,
        installations: Arc<dyn InstallationsTokenProvider>,
    ) -> Self {
        Self {
            client,
            base_url: base_url.into(),
            project_id: project_id.into(),
            namespace: namespace.into(),
            api_key: api_key.into(),
            app_id: app_id.into(),
            sdk_version: sdk_version.into(),
            language_code: language_code.into(),
            installations,
        }
    }

    fn request_body(
        &self,
        installation_id: String,
        installation_token: String,
        custom_signals: Option<HashMap<String, JsonValue>>,
    ) -> JsonValue {
        let mut payload = json!({
            "sdk_version": self.sdk_version,
            "app_instance_id": installation_id,
            "app_instance_id_token": installation_token,
            "app_id": self.app_id,
            "language_code": self.language_code,
        });

        if let Some(signals) = custom_signals {
            if let Some(obj) = payload.as_object_mut() {
                let mut map = JsonMap::with_capacity(signals.len());
                for (key, value) in signals {
                    map.insert(key, value);
                }
                obj.insert("custom_signals".to_string(), JsonValue::Object(map));
            }
        }

        payload
    }

    fn build_url(&self) -> String {
        format!(
            "{}/v1/projects/{}/namespaces/{}:fetch?key={}",
            self.base_url, self.project_id, self.namespace, self.api_key
        )
    }
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
#[async_trait::async_trait(?Send)]
impl RemoteConfigFetchClient for WasmRemoteConfigFetchClient {
    async fn fetch(&self, request: FetchRequest) -> RemoteConfigResult<FetchResponse> {
        let installation_id = map_installations_error(self.installations.installation_id().await)?;
        let installation_token = map_installations_error(self.installations.installation_token().await)?;
        let url = self.build_url();

        let body = self.request_body(installation_id, installation_token, request.custom_signals);

        let response = self
            .client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|err| internal_error(format!("remote config fetch failed: {err}")))?;

        let mut status = response.status();
        let e_tag = response
            .headers()
            .get("ETag")
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string());

        let response_body = if status == StatusCode::OK {
            Some(
                response
                    .json::<RestFetchResponse>()
                    .await
                    .map_err(|err| internal_error(format!("failed to parse Remote Config response: {err}")))?,
            )
        } else if status == StatusCode::NOT_MODIFIED {
            None
        } else {
            return Err(internal_error(format!("fetch returned unexpected status {}", status.as_u16())));
        };

        let mut config = response_body.as_ref().and_then(|body| body.entries.clone());
        let state = response_body.as_ref().and_then(|body| body.state.clone());
        let template_version = response_body.as_ref().and_then(|body| body.template_version);

        match state.as_deref() {
            Some("INSTANCE_STATE_UNSPECIFIED") => status = StatusCode::INTERNAL_SERVER_ERROR,
            Some("NO_CHANGE") => status = StatusCode::NOT_MODIFIED,
            Some("NO_TEMPLATE") | Some("EMPTY_CONFIG") => {
                config = Some(HashMap::new());
            }
            _ => {}
        }

        match status {
            StatusCode::OK | StatusCode::NOT_MODIFIED => Ok(FetchResponse {
                status: status.as_u16(),
                etag: e_tag,
                config,
                template_version,
            }),
            other => Err(internal_error(format!("fetch returned unexpected status {}", other.as_u16()))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct TestInstallations {
        installation_id: String,
        installation_token: String,
        id_calls: AtomicUsize,
        token_calls: AtomicUsize,
    }

    impl TestInstallations {
        fn new(id: &str, token: &str) -> Self {
            Self {
                installation_id: id.to_string(),
                installation_token: token.to_string(),
                id_calls: AtomicUsize::new(0),
                token_calls: AtomicUsize::new(0),
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        fn id_call_count(&self) -> usize {
            self.id_calls.load(Ordering::SeqCst)
        }

        #[cfg(not(target_arch = "wasm32"))]
        fn token_call_count(&self) -> usize {
            self.token_calls.load(Ordering::SeqCst)
        }
    }

    #[cfg_attr(
        all(feature = "wasm-web", target_arch = "wasm32"),
        async_trait::async_trait(?Send)
    )]
    #[cfg_attr(not(all(feature = "wasm-web", target_arch = "wasm32")), async_trait::async_trait)]
    impl InstallationsTokenProvider for TestInstallations {
        async fn installation_id(&self) -> InstallationsResult<String> {
            self.id_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.installation_id.clone())
        }

        async fn installation_token(&self) -> InstallationsResult<String> {
            self.token_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.installation_token.clone())
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    mod native {
        use super::*;
        use httpmock::prelude::*;
        use serde_json::json;
        fn fetch_request() -> FetchRequest {
            let mut signals = HashMap::new();
            signals.insert("feature".to_string(), JsonValue::Bool(true));
            FetchRequest {
                cache_max_age_millis: 60_000,
                timeout_millis: 5_000,
                e_tag: Some("\"etag-value\"".to_string()),
                custom_signals: Some(signals),
            }
        }

        #[tokio::test(flavor = "current_thread")]
        async fn http_fetch_client_returns_config() {
            let server = MockServer::start();
            let mock = server.mock(|when, then| {
                when.method(POST)
                    .path("/v1/projects/test-project/namespaces/test-namespace:fetch")
                    .header("content-type", "application/json")
                    .header("if-none-match", "\"etag-value\"")
                    .json_body(json!({
                        "sdk_version": "test-sdk",
                        "app_instance_id": "test-installation",
                        "app_instance_id_token": "test-token",
                        "app_id": "test-app",
                        "language_code": "en-GB",
                        "custom_signals": { "feature": true }
                    }));
                then.status(200).header("ETag", "\"new-etag\"").json_body(json!({
                    "entries": { "welcome": "hello" },
                    "templateVersion": 42u64
                }));
            });

            let provider = Arc::new(TestInstallations::new("test-installation", "test-token"));
            let client = HttpRemoteConfigFetchClient::new(
                Client::builder().build().unwrap(),
                server.base_url(),
                "test-project",
                "test-namespace",
                "test-api-key",
                "test-app",
                "test-sdk",
                "en-GB",
                provider.clone(),
            );

            let response = client.fetch(fetch_request()).await.expect("fetch succeeds");
            mock.assert();

            assert_eq!(response.status, 200);
            assert_eq!(response.etag.as_deref(), Some("\"new-etag\""));
            assert_eq!(response.template_version, Some(42));
            let config = response.config.expect("config present");
            assert_eq!(config.get("welcome"), Some(&"hello".to_string()));

            assert_eq!(provider.id_call_count(), 1);
            assert_eq!(provider.token_call_count(), 1);
        }

        #[tokio::test(flavor = "current_thread")]
        async fn http_fetch_client_handles_not_modified() {
            let server = MockServer::start();
            let mock = server.mock(|when, then| {
                when.method(POST)
                    .path("/v1/projects/test-project/namespaces/test-namespace:fetch");
                then.status(304);
            });

            let provider = Arc::new(TestInstallations::new("test-installation", "test-token"));
            let client = HttpRemoteConfigFetchClient::new(
                Client::builder().build().unwrap(),
                server.base_url(),
                "test-project",
                "test-namespace",
                "test-api-key",
                "test-app",
                "test-sdk",
                "en-US",
                provider.clone(),
            );

            let mut request = fetch_request();
            request.custom_signals = None;
            let response = client.fetch(request).await.expect("fetch succeeds");
            mock.assert();

            assert_eq!(response.status, 304);
            assert!(response.config.is_none());
            assert_eq!(provider.id_call_count(), 1);
            assert_eq!(provider.token_call_count(), 1);
        }

        #[tokio::test(flavor = "current_thread")]
        async fn http_fetch_client_surfaces_server_errors() {
            let server = MockServer::start();
            let mock = server.mock(|when, then| {
                when.method(POST)
                    .path("/v1/projects/test-project/namespaces/test-namespace:fetch");
                then.status(503).body("unavailable");
            });

            let provider = Arc::new(TestInstallations::new("test-installation", "test-token"));
            let client = HttpRemoteConfigFetchClient::new(
                Client::builder().build().unwrap(),
                server.base_url(),
                "test-project",
                "test-namespace",
                "test-api-key",
                "test-app",
                "test-sdk",
                "en-US",
                provider.clone(),
            );

            let result = client.fetch(fetch_request()).await;
            mock.assert();
            assert!(result.is_err());
            assert_eq!(provider.id_call_count(), 1);
            assert_eq!(provider.token_call_count(), 1);
        }
    }

    #[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
    mod wasm {
        use super::*;
        use serde_json::json;
        use wasm_bindgen_test::wasm_bindgen_test;

        wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

        #[wasm_bindgen_test]
        fn request_body_includes_custom_signals() {
            let provider = Arc::new(TestInstallations::new("id", "token"));
            let client = WasmRemoteConfigFetchClient::new(
                Client::new(),
                "https://example.com",
                "test-project",
                "test-namespace",
                "test-api-key",
                "test-app",
                "test-sdk",
                "fr-FR",
                provider,
            );

            let mut signals = HashMap::new();
            signals.insert("flag".to_string(), JsonValue::Bool(true));

            let body = client.request_body("iid".into(), "itoken".into(), Some(signals));
            assert_eq!(body["language_code"], json!("fr-FR"));
            assert_eq!(body["custom_signals"].get("flag"), Some(&json!(true)));
        }
    }
}
