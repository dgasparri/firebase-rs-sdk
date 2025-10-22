use std::time::Duration;

use serde_json::Value;
use url::Url;

use crate::ai::backend::Backend;
use crate::ai::constants::{
    DEFAULT_API_VERSION, DEFAULT_DOMAIN, DEFAULT_FETCH_TIMEOUT_MS, LANGUAGE_TAG, PACKAGE_VERSION,
};
use crate::ai::error::{AiError, AiErrorCode, AiResult};

/// Internal settings required to build REST requests.
///
/// Mirrors `ApiSettings` from `packages/ai/src/types/internal.ts`.
#[derive(Clone, Debug)]
pub(crate) struct ApiSettings {
    pub api_key: String,
    pub project: String,
    pub app_id: String,
    pub backend: Backend,
    pub automatic_data_collection_enabled: bool,
    pub app_check_token: Option<String>,
    pub auth_token: Option<String>,
}

impl ApiSettings {
    pub fn new(
        api_key: String,
        project: String,
        app_id: String,
        backend: Backend,
        automatic_data_collection_enabled: bool,
        app_check_token: Option<String>,
        auth_token: Option<String>,
    ) -> Self {
        Self {
            api_key,
            project,
            app_id,
            backend,
            automatic_data_collection_enabled,
            app_check_token,
            auth_token,
        }
    }
}

/// Additional per-request options.
///
/// Ported from `packages/ai/src/types/requests.ts` (`RequestOptions`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RequestOptions {
    /// Optional request timeout. Defaults to 180 seconds when omitted.
    pub timeout: Option<Duration>,
    /// Optional base URL overriding the default Firebase AI endpoint.
    pub base_url: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpMethod {
    Post,
}

/// Prepared HTTP request ready to be executed by an HTTP client.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedRequest {
    pub method: HttpMethod,
    pub url: Url,
    pub headers: Vec<(String, String)>,
    pub body: Value,
    pub timeout: Duration,
}

impl PreparedRequest {
    /// Returns the value of a header if it exists.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// Converts the prepared request into a `reqwest::RequestBuilder`.
    ///
    /// This helper is only compiled when the `ai-http` feature is enabled so the core library
    /// remains network agnostic.
    #[cfg(feature = "ai-http")]
    pub fn into_reqwest(
        self,
        client: &reqwest::Client,
    ) -> Result<reqwest::RequestBuilder, AiError> {
        use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

        let mut headers = HeaderMap::new();
        for (name, value) in &self.headers {
            let header_name = HeaderName::from_bytes(name.as_bytes()).map_err(|err| {
                AiError::new(
                    AiErrorCode::InvalidArgument,
                    format!("Invalid header name '{name}': {err}"),
                    None,
                )
            })?;
            let header_value = HeaderValue::from_str(value).map_err(|err| {
                AiError::new(
                    AiErrorCode::InvalidArgument,
                    format!("Invalid header value for '{name}': {err}"),
                    None,
                )
            })?;
            headers.insert(header_name, header_value);
        }

        let builder = match self.method {
            HttpMethod::Post => client.post(self.url.clone()),
        }
        .headers(headers)
        .timeout(self.timeout)
        .body(self.body.to_string());
        Ok(builder)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RequestFactory {
    settings: ApiSettings,
}

impl RequestFactory {
    pub fn new(settings: ApiSettings) -> Self {
        Self { settings }
    }

    pub fn construct_request(
        &self,
        model: &str,
        task: Task,
        stream: bool,
        body: Value,
        request_options: Option<RequestOptions>,
    ) -> AiResult<PreparedRequest> {
        let options = request_options.unwrap_or_default();
        let mut url = self.compose_base_url(&options)?;
        let trimmed_model = model.trim_start_matches('/');
        let model_path = match &self.settings.backend {
            Backend::GoogleAi(_) => format!("projects/{}/{}", self.settings.project, trimmed_model),
            Backend::VertexAi(inner) => format!(
                "projects/{}/locations/{}/{}",
                self.settings.project,
                inner.location(),
                trimmed_model
            ),
        };
        let path = format!(
            "/{}/{model_path}:{}",
            DEFAULT_API_VERSION,
            task.as_operation()
        );
        url.set_path(&path);
        if stream {
            url.query_pairs_mut().append_pair("alt", "sse");
        } else {
            url.set_query(None);
        }

        let timeout = options
            .timeout
            .unwrap_or_else(|| Duration::from_millis(DEFAULT_FETCH_TIMEOUT_MS));
        let headers = self.build_headers();

        Ok(PreparedRequest {
            method: HttpMethod::Post,
            url,
            headers,
            body,
            timeout,
        })
    }

    fn compose_base_url(&self, options: &RequestOptions) -> AiResult<Url> {
        let base = options
            .base_url
            .as_ref()
            .map(|value| value.as_str())
            .unwrap_or(DEFAULT_DOMAIN);
        let url = if base.starts_with("http://") || base.starts_with("https://") {
            base.to_string()
        } else {
            format!("https://{base}")
        };
        Url::parse(&url).map_err(|err| {
            AiError::new(
                AiErrorCode::InvalidArgument,
                format!("Invalid base URL '{url}': {err}"),
                None,
            )
        })
    }

    fn build_headers(&self) -> Vec<(String, String)> {
        let mut headers = Vec::with_capacity(5);
        headers.push(("Content-Type".into(), "application/json".into()));
        let client_header = format!(
            "{}/{} fire/{}",
            LANGUAGE_TAG, PACKAGE_VERSION, PACKAGE_VERSION
        );
        headers.push(("x-goog-api-client".into(), client_header));
        headers.push(("x-goog-api-key".into(), self.settings.api_key.clone()));

        if self.settings.automatic_data_collection_enabled && !self.settings.app_id.is_empty() {
            headers.push(("X-Firebase-AppId".into(), self.settings.app_id.clone()));
        }

        if let Some(token) = &self.settings.app_check_token {
            headers.push(("X-Firebase-AppCheck".into(), token.clone()));
        }

        if let Some(token) = &self.settings.auth_token {
            headers.push(("Authorization".into(), format!("Firebase {token}")));
        }

        headers
    }
}

/// High-level tasks supported by the request factory.
///
/// Mirrors the `Task` enum in `packages/ai/src/requests/request.ts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Task {
    GenerateContent,
    CountTokens,
    Predict,
}

impl Task {
    pub fn as_operation(&self) -> &'static str {
        match self {
            Task::GenerateContent => "generateContent",
            Task::CountTokens => "countTokens",
            Task::Predict => "predict",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with_backend(backend: Backend) -> ApiSettings {
        ApiSettings::new(
            "test-key".into(),
            "test-project".into(),
            "1:123:web:abc".into(),
            backend,
            true,
            None,
            None,
        )
    }

    #[test]
    fn constructs_google_ai_url() {
        let factory = RequestFactory::new(settings_with_backend(Backend::google_ai()));
        let req = factory
            .construct_request(
                "models/gemini-1.5-flash",
                Task::GenerateContent,
                false,
                serde_json::json!({"contents": []}),
                None,
            )
            .unwrap();
        assert_eq!(
            req.url.as_str(),
            "https://firebasevertexai.googleapis.com/v1beta/projects/test-project/models/gemini-1.5-flash:generateContent"
        );
        assert_eq!(req.header("x-goog-api-key"), Some("test-key"));
        assert_eq!(req.timeout, Duration::from_millis(DEFAULT_FETCH_TIMEOUT_MS));
    }

    #[test]
    fn constructs_vertex_ai_url_with_base_override() {
        let factory =
            RequestFactory::new(settings_with_backend(Backend::vertex_ai("europe-west4")));
        let options = RequestOptions {
            timeout: Some(Duration::from_secs(10)),
            base_url: Some("https://example.com".into()),
        };
        let req = factory
            .construct_request(
                "models/gemini-pro",
                Task::CountTokens,
                false,
                serde_json::json!({"contents": []}),
                Some(options),
            )
            .unwrap();
        assert_eq!(
            req.url.as_str(),
            "https://example.com/v1beta/projects/test-project/locations/europe-west4/models/gemini-pro:countTokens"
        );
        assert_eq!(req.timeout, Duration::from_secs(10));
    }

    #[test]
    fn invalid_base_url_returns_error() {
        let factory = RequestFactory::new(settings_with_backend(Backend::google_ai()));
        let err = factory
            .construct_request(
                "models/test",
                Task::Predict,
                false,
                serde_json::json!({}),
                Some(RequestOptions {
                    timeout: None,
                    base_url: Some("://bad".into()),
                }),
            )
            .unwrap_err();
        assert_eq!(err.code(), AiErrorCode::InvalidArgument);
    }
}
