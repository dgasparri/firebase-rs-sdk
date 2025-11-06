use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, LazyLock, Mutex};

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::ai::backend::{Backend, BackendType};
use crate::ai::constants::AI_COMPONENT_NAME;
use crate::ai::error::{
    internal_error, invalid_argument, AiError, AiErrorCode, AiResult, CustomErrorData,
};
use crate::ai::helpers::{decode_instance_identifier, encode_instance_identifier};
use crate::ai::public_types::{AiOptions, AiRuntimeOptions};
use crate::ai::requests::{ApiSettings, PreparedRequest, RequestFactory, RequestOptions, Task};
use crate::app;
use crate::app::{FirebaseApp, FirebaseOptions};
use crate::app_check::FirebaseAppCheckInternal;
use crate::auth::Auth;
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType, Provider};

#[derive(Clone)]
pub struct AiService {
    inner: Arc<AiInner>,
}

impl fmt::Debug for AiService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AiService")
            .field("app", &self.inner.app.name())
            .field("backend", &self.inner.backend.backend_type())
            .finish()
    }
}

struct AiInner {
    app: FirebaseApp,
    backend: Backend,
    options: Mutex<AiRuntimeOptions>,
    default_model: Option<String>,
    auth_provider: Provider,
    app_check_provider: Provider,
    transport: Mutex<Arc<dyn AiHttpTransport>>,
    #[cfg(test)]
    test_tokens: Mutex<TestTokenOverrides>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct CacheKey {
    app_name: String,
    identifier: String,
}

impl CacheKey {
    fn new(app_name: &str, identifier: &str) -> Self {
        Self {
            app_name: app_name.to_string(),
            identifier: identifier.to_string(),
        }
    }
}

static AI_OVERRIDES: LazyLock<Mutex<HashMap<CacheKey, Arc<AiService>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
trait AiHttpTransport: Send + Sync {
    async fn send(&self, request: PreparedRequest) -> AiResult<Value>;
}

struct ReqwestTransport {
    client: reqwest::Client,
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[cfg(test)]
#[derive(Default)]
struct TestTokenOverrides {
    auth: Option<String>,
    app_check: Option<String>,
    limited_app_check: Option<String>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl AiHttpTransport for ReqwestTransport {
    async fn send(&self, request: PreparedRequest) -> AiResult<Value> {
        let builder = request
            .into_reqwest(&self.client)
            .map_err(|err| internal_error(format!("failed to encode AI request: {err}")))?;

        let response = builder.send().await.map_err(|err| {
            AiError::new(
                AiErrorCode::FetchError,
                format!("failed to send AI request: {err}"),
                None,
            )
        })?;

        let status = response.status();
        let bytes = response.bytes().await.map_err(|err| {
            AiError::new(
                AiErrorCode::FetchError,
                format!("failed to read AI response body: {err}"),
                None,
            )
        })?;

        let parsed = serde_json::from_slice::<Value>(&bytes);

        if !status.is_success() {
            let mut data = CustomErrorData::default().with_status(status.as_u16());
            if let Some(reason) = status.canonical_reason() {
                data = data.with_status_text(reason);
            }

            return match parsed {
                Ok(json) => {
                    let message = AiService::extract_error_message(&json)
                        .unwrap_or_else(|| format!("AI endpoint returned HTTP {status}"));
                    Err(AiError::new(
                        AiErrorCode::FetchError,
                        message,
                        Some(data.with_response(json)),
                    ))
                }
                Err(_) => {
                    let raw = String::from_utf8_lossy(&bytes).to_string();
                    Err(AiError::new(
                        AiErrorCode::FetchError,
                        format!("AI endpoint returned HTTP {status}"),
                        Some(data.with_response(json!({ "raw": raw }))),
                    ))
                }
            };
        }

        parsed.map_err(|err| {
            AiError::new(
                AiErrorCode::ParseFailed,
                format!("failed to parse AI response JSON: {err}"),
                None,
            )
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GenerateTextRequest {
    pub prompt: String,
    pub model: Option<String>,
    pub request_options: Option<RequestOptions>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerateTextResponse {
    pub text: String,
    pub model: String,
}

impl AiService {
    fn new(
        app: FirebaseApp,
        backend: Backend,
        options: AiRuntimeOptions,
        default_model: Option<String>,
        auth_provider: Provider,
        app_check_provider: Provider,
    ) -> Self {
        Self {
            inner: Arc::new(AiInner {
                app,
                backend,
                options: Mutex::new(options),
                default_model,
                auth_provider,
                app_check_provider,
                transport: Mutex::new(Arc::new(ReqwestTransport::default())),
                #[cfg(test)]
                test_tokens: Mutex::new(TestTokenOverrides::default()),
            }),
        }
    }

    /// Returns the Firebase app associated with this AI service.
    pub fn app(&self) -> &FirebaseApp {
        &self.inner.app
    }

    /// Returns the backend configuration used by this AI service.
    pub fn backend(&self) -> &Backend {
        &self.inner.backend
    }

    /// Returns the backend type tag.
    pub fn backend_type(&self) -> BackendType {
        self.inner.backend.backend_type()
    }

    /// Returns the Vertex AI location when using that backend.
    pub fn location(&self) -> Option<&str> {
        self.inner
            .backend
            .as_vertex_ai()
            .map(|backend| backend.location())
    }

    /// Returns the runtime options currently applied to this AI service.
    pub fn options(&self) -> AiRuntimeOptions {
        self.inner.options.lock().unwrap().clone()
    }

    fn set_options(&self, options: AiRuntimeOptions) {
        *self.inner.options.lock().unwrap() = options;
    }

    #[cfg(test)]
    fn set_transport_for_tests(&self, transport: Arc<dyn AiHttpTransport>) {
        *self.inner.transport.lock().unwrap() = transport;
    }

    #[cfg(test)]
    fn override_tokens_for_tests(
        &self,
        auth: Option<String>,
        app_check: Option<String>,
        limited_app_check: Option<String>,
    ) {
        let mut overrides = self.inner.test_tokens.lock().unwrap();
        overrides.auth = auth;
        overrides.app_check = app_check;
        overrides.limited_app_check = limited_app_check;
    }

    async fn fetch_auth_token(&self) -> AiResult<Option<String>> {
        #[cfg(test)]
        if let Some(token) = self.inner.test_tokens.lock().unwrap().auth.clone() {
            return Ok(Some(token));
        }

        let auth = match self
            .inner
            .auth_provider
            .get_immediate_with_options::<Auth>(None, true)
        {
            Ok(Some(auth)) => auth,
            Ok(None) => return Ok(None),
            Err(err) => {
                return Err(internal_error(format!(
                    "failed to resolve auth provider: {err}"
                )))
            }
        };

        match auth.get_token(false).await {
            Ok(Some(token)) if token.is_empty() => Ok(None),
            Ok(Some(token)) => Ok(Some(token)),
            Ok(None) => Ok(None),
            Err(err) => Err(internal_error(format!(
                "failed to obtain auth token: {err}"
            ))),
        }
    }

    async fn fetch_app_check_credentials(
        &self,
        limited_use: bool,
    ) -> AiResult<(Option<String>, Option<String>)> {
        #[cfg(test)]
        {
            let overrides = self.inner.test_tokens.lock().unwrap();
            if limited_use {
                if let Some(token) = overrides.limited_app_check.clone() {
                    return Ok((Some(token), None));
                }
            } else if let Some(token) = overrides.app_check.clone() {
                return Ok((Some(token), None));
            }
        }

        let app_check = match self
            .inner
            .app_check_provider
            .get_immediate_with_options::<FirebaseAppCheckInternal>(None, true)
        {
            Ok(Some(app_check)) => app_check,
            Ok(None) => return Ok((None, None)),
            Err(err) => {
                return Err(internal_error(format!(
                    "failed to resolve App Check provider: {err}"
                )))
            }
        };

        let token = match if limited_use {
            app_check.get_limited_use_token().await
        } else {
            app_check.get_token(false).await
        } {
            Ok(result) => Ok(result.token),
            Err(err) => err
                .cached_token()
                .map(|cached| cached.token.clone())
                .ok_or_else(|| internal_error(format!("failed to obtain App Check token: {err}"))),
        }?;

        if token.is_empty() {
            return Ok((None, None));
        }

        let heartbeat = app_check.heartbeat_header().await.map_err(|err| {
            internal_error(format!(
                "failed to obtain App Check heartbeat header: {err}"
            ))
        })?;

        Ok((Some(token), heartbeat))
    }

    pub(crate) async fn api_settings(&self) -> AiResult<ApiSettings> {
        let options = self.inner.app.options();
        let FirebaseOptions {
            api_key,
            project_id,
            app_id,
            ..
        } = options;

        let api_key = api_key.ok_or_else(|| {
            AiError::new(
                AiErrorCode::NoApiKey,
                "Firebase options must include `api_key` to use Firebase AI endpoints",
                None,
            )
        })?;
        let project_id = project_id.ok_or_else(|| {
            AiError::new(
                AiErrorCode::NoProjectId,
                "Firebase options must include `project_id` to use Firebase AI endpoints",
                None,
            )
        })?;
        let app_id = app_id.ok_or_else(|| {
            AiError::new(
                AiErrorCode::NoAppId,
                "Firebase options must include `app_id` to use Firebase AI endpoints",
                None,
            )
        })?;

        let runtime_options = self.options();
        let automatic = self.inner.app.automatic_data_collection_enabled();
        let (app_check_token, app_check_heartbeat) = self
            .fetch_app_check_credentials(runtime_options.use_limited_use_app_check_tokens)
            .await?;
        let auth_token = self.fetch_auth_token().await?;

        Ok(ApiSettings::new(
            api_key,
            project_id,
            app_id,
            self.inner.backend.clone(),
            automatic,
            app_check_token,
            app_check_heartbeat,
            auth_token,
        ))
    }

    pub(crate) async fn request_factory(&self) -> AiResult<RequestFactory> {
        Ok(RequestFactory::new(self.api_settings().await?))
    }

    /// Prepares a REST request for a `generateContent` call without executing it.
    ///
    /// This mirrors the behaviour of `constructRequest` in the TypeScript SDK and allows advanced
    /// callers to integrate with custom HTTP stacks while the SDK handles URL/header generation.
    pub async fn prepare_generate_content_request(
        &self,
        model: &str,
        body: Value,
        request_options: Option<RequestOptions>,
    ) -> AiResult<PreparedRequest> {
        let factory = self.request_factory().await?;
        factory.construct_request(model, Task::GenerateContent, false, body, request_options)
    }

    /// Generates text using the configured backend.
    ///
    /// This issues a `generateContent` REST call against the active backend, attaching
    /// auth and App Check credentials when available. The optional
    /// [`RequestOptions`] can override the base URL or timeout, which is primarily
    /// intended for tests and emulator scenarios.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use firebase_rs_sdk::ai::{AiService, GenerateTextRequest};
    /// # async fn example(ai: AiService) -> firebase_rs_sdk::ai::error::AiResult<()> {
    /// let response = ai
    ///     .generate_text(GenerateTextRequest {
    ///         prompt: "Hello Gemini".to_owned(),
    ///         model: None,
    ///         request_options: None,
    ///     })
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn generate_text(
        &self,
        request: GenerateTextRequest,
    ) -> AiResult<GenerateTextResponse> {
        if request.prompt.trim().is_empty() {
            return Err(invalid_argument("Prompt must not be empty"));
        }
        let model = request
            .model
            .or_else(|| self.inner.default_model.clone())
            .unwrap_or_else(|| "text-bison-001".to_string());

        let body = Self::build_generate_text_body(&request.prompt);
        let prepared = self
            .prepare_generate_content_request(&model, body, request.request_options.clone())
            .await?;
        let response = self.execute_prepared_request(prepared).await?;
        let text = match Self::extract_text_from_response(&response) {
            Some(text) => text,
            None => {
                return Err(AiError::new(
                    AiErrorCode::ResponseError,
                    "AI response did not contain textual content",
                    Some(CustomErrorData::default().with_response(response)),
                ))
            }
        };

        Ok(GenerateTextResponse { text, model })
    }

    async fn execute_prepared_request(&self, prepared: PreparedRequest) -> AiResult<Value> {
        let transport = self.inner.transport.lock().unwrap().clone();
        transport.send(prepared).await
    }

    fn build_generate_text_body(prompt: &str) -> Value {
        json!({
            "contents": [
                {
                    "role": "user",
                    "parts": [
                        {
                            "text": prompt,
                        }
                    ]
                }
            ]
        })
    }

    fn extract_text_from_response(response: &Value) -> Option<String> {
        if let Some(candidates) = response
            .get("candidates")
            .and_then(|value| value.as_array())
        {
            for candidate in candidates {
                if let Some(text) = Self::extract_text_from_candidate(candidate) {
                    if !text.trim().is_empty() {
                        return Some(text);
                    }
                }
            }
        }

        response
            .get("output")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
    }

    fn extract_text_from_candidate(candidate: &Value) -> Option<String> {
        if let Some(content) = candidate.get("content") {
            if let Some(parts) = content.get("parts").and_then(|value| value.as_array()) {
                for part in parts {
                    if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
                        if !text.is_empty() {
                            return Some(text.to_string());
                        }
                    }
                }
            }
        }

        candidate
            .get("output")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
    }

    fn extract_error_message(value: &Value) -> Option<String> {
        if let Some(error) = value.get("error") {
            if let Some(message) = error.get("message").and_then(|v| v.as_str()) {
                return Some(message.to_string());
            }
        }

        value
            .get("message")
            .and_then(|v| v.as_str())
            .map(|message| message.to_string())
    }
}

#[derive(Debug)]
struct Cache;

impl Cache {
    fn get(key: &CacheKey) -> Option<Arc<AiService>> {
        AI_OVERRIDES.lock().unwrap().get(key).cloned()
    }

    fn insert(key: CacheKey, service: Arc<AiService>) {
        AI_OVERRIDES.lock().unwrap().insert(key, service);
    }
}

static AI_COMPONENT: LazyLock<()> = LazyLock::new(|| {
    let component = Component::new(
        AI_COMPONENT_NAME,
        Arc::new(ai_factory),
        ComponentType::Public,
    )
    .with_instantiation_mode(InstantiationMode::Lazy)
    .with_multiple_instances(true);
    let _ = app::register_component(component);
});

fn ai_factory(
    container: &crate::component::ComponentContainer,
    options: InstanceFactoryOptions,
) -> Result<DynService, ComponentError> {
    let app = container.root_service::<FirebaseApp>().ok_or_else(|| {
        ComponentError::InitializationFailed {
            name: AI_COMPONENT_NAME.to_string(),
            reason: "Firebase app not attached to component container".to_string(),
        }
    })?;

    let identifier_backend = options
        .instance_identifier
        .as_deref()
        .map(|identifier| decode_instance_identifier(identifier));

    let auth_provider = container.get_provider("auth-internal");
    let app_check_provider = container.get_provider("app-check-internal");

    let backend = match identifier_backend {
        Some(Ok(backend)) => backend,
        Some(Err(err)) => {
            return Err(ComponentError::InitializationFailed {
                name: AI_COMPONENT_NAME.to_string(),
                reason: err.to_string(),
            })
        }
        None => {
            if let Some(encoded) = options
                .options
                .get("backend")
                .and_then(|value| value.as_str())
            {
                decode_instance_identifier(encoded).map_err(|err| {
                    ComponentError::InitializationFailed {
                        name: AI_COMPONENT_NAME.to_string(),
                        reason: err.to_string(),
                    }
                })?
            } else {
                Backend::default()
            }
        }
    };

    let use_limited_tokens = options
        .options
        .get("useLimitedUseAppCheckTokens")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let runtime_options = AiRuntimeOptions {
        use_limited_use_app_check_tokens: use_limited_tokens,
    };

    let default_model = options
        .options
        .get("defaultModel")
        .and_then(|value| value.as_str().map(|s| s.to_string()));

    let service = AiService::new(
        (*app).clone(),
        backend,
        runtime_options,
        default_model,
        auth_provider,
        app_check_provider,
    );
    Ok(Arc::new(service) as DynService)
}

fn ensure_registered() {
    LazyLock::force(&AI_COMPONENT);
}

/// Registers the AI component in the global registry.
pub fn register_ai_component() {
    ensure_registered();
}

/// Returns an AI service instance, mirroring the JavaScript `getAI()` API.
///
/// When `options` is provided the backend identifier is encoded using the same
/// rules as `encodeInstanceIdentifier` from the JavaScript SDK so that separate
/// backend configurations create independent service instances.
///
/// # Examples
///
/// ```
/// # use firebase_rs_sdk::ai::backend::Backend;
/// # use firebase_rs_sdk::ai::public_types::AiOptions;
/// # use firebase_rs_sdk::ai::get_ai;
/// # use firebase_rs_sdk::app::initialize_app;
/// # use firebase_rs_sdk::app::{FirebaseAppSettings, FirebaseOptions};
/// # async fn example() {
/// let options = FirebaseOptions {
///     project_id: Some("project".into()),
///     api_key: Some("test".into()),
///     ..Default::default()
/// };
/// let app = initialize_app(options, Some(FirebaseAppSettings::default())).await.unwrap();
/// let ai = get_ai(
///     Some(app),
///     Some(AiOptions {
///         backend: Some(Backend::vertex_ai("us-central1")),
///         use_limited_use_app_check_tokens: Some(false),
///     }),
/// )
/// .await
/// .unwrap();
/// # }
/// ```
pub async fn get_ai(
    app: Option<FirebaseApp>,
    options: Option<AiOptions>,
) -> AiResult<Arc<AiService>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => crate::app::get_app(None)
            .await
            .map_err(|err| internal_error(err.to_string()))?,
    };

    let options = options.unwrap_or_default();
    let backend = options.backend_or_default();
    let identifier = encode_instance_identifier(&backend);
    let runtime_options = AiRuntimeOptions {
        use_limited_use_app_check_tokens: options.limited_use_app_check(),
    };

    let cache_key = CacheKey::new(app.name(), &identifier);
    if let Some(service) = Cache::get(&cache_key) {
        service.set_options(runtime_options.clone());
        return Ok(service);
    }

    let provider = app::get_provider(&app, AI_COMPONENT_NAME);

    if let Some(service) = provider
        .get_immediate_with_options::<AiService>(Some(&identifier), true)
        .map_err(|err| internal_error(err.to_string()))?
    {
        service.set_options(runtime_options.clone());
        Cache::insert(cache_key.clone(), service.clone());
        return Ok(service);
    }

    match provider.initialize::<AiService>(
        json!({
            "backend": identifier,
            "useLimitedUseAppCheckTokens": runtime_options.use_limited_use_app_check_tokens,
        }),
        Some(&cache_key.identifier),
    ) {
        Ok(service) => {
            service.set_options(runtime_options.clone());
            Cache::insert(cache_key.clone(), service.clone());
            Ok(service)
        }
        Err(ComponentError::InstanceUnavailable { .. }) => {
            if let Some(service) = provider
                .get_immediate_with_options::<AiService>(Some(&cache_key.identifier), true)
                .map_err(|err| internal_error(err.to_string()))?
            {
                service.set_options(runtime_options.clone());
                Cache::insert(cache_key.clone(), service.clone());
                Ok(service)
            } else {
                let container = app.container();
                let fallback = Arc::new(AiService::new(
                    app.clone(),
                    backend,
                    runtime_options,
                    None,
                    container.get_provider("auth-internal"),
                    container.get_provider("app-check-internal"),
                ));
                Cache::insert(cache_key.clone(), fallback.clone());
                Ok(fallback)
            }
        }
        Err(err) => Err(internal_error(err.to_string())),
    }
}

/// Convenience wrapper that mirrors the original Rust stub signature.
pub async fn get_ai_service(app: Option<FirebaseApp>) -> AiResult<Arc<AiService>> {
    get_ai(app, None).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::backend::Backend;
    use crate::ai::error::AiErrorCode;
    use crate::ai::public_types::AiOptions;
    use crate::app::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use async_trait::async_trait;
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn unique_settings() -> FirebaseAppSettings {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!("ai-{}", COUNTER.fetch_add(1, Ordering::SeqCst))),
            ..Default::default()
        }
    }

    #[derive(Clone, Default)]
    struct TestTransport {
        responses: Arc<Mutex<VecDeque<AiResult<Value>>>>,
        requests: Arc<Mutex<Vec<PreparedRequest>>>,
    }

    impl TestTransport {
        fn new() -> Self {
            Self::default()
        }

        fn push_response(&self, response: AiResult<Value>) {
            self.responses.lock().unwrap().push_back(response);
        }

        fn take_requests(&self) -> Vec<PreparedRequest> {
            self.requests.lock().unwrap().clone()
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl AiHttpTransport for TestTransport {
        async fn send(&self, request: PreparedRequest) -> AiResult<Value> {
            self.requests.lock().unwrap().push(request);
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Err(internal_error("no stub response configured")))
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn generate_text_includes_backend_info() {
        let transport = TestTransport::new();
        transport.push_response(Ok(json!({
            "candidates": [
                {
                    "content": {
                        "parts": [
                            { "text": "Hello from mock" }
                        ]
                    }
                }
            ]
        })));

        let options = FirebaseOptions {
            project_id: Some("project".into()),
            api_key: Some("api".into()),
            app_id: Some("app".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .unwrap();
        let ai = get_ai_service(Some(app)).await.unwrap();
        ai.set_transport_for_tests(Arc::new(transport.clone()));
        let response = ai
            .generate_text(GenerateTextRequest {
                prompt: "Hello AI".to_string(),
                model: Some("models/gemini-pro".to_string()),
                request_options: None,
            })
            .await
            .unwrap();

        assert_eq!(response.model, "models/gemini-pro");
        assert_eq!(response.text, "Hello from mock");

        let requests = transport.take_requests();
        assert_eq!(requests.len(), 1);
        assert!(requests[0]
            .url
            .as_str()
            .ends_with("models/gemini-pro:generateContent"));
        assert_eq!(requests[0].header("x-goog-api-key"), Some("api"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn limited_use_app_check_token_attached_to_requests() {
        let transport = TestTransport::new();
        transport.push_response(Ok(json!({
            "candidates": [
                {
                    "content": {
                        "parts": [
                            { "text": "Limited token response" }
                        ]
                    }
                }
            ]
        })));

        let options = FirebaseOptions {
            project_id: Some("project".into()),
            api_key: Some("api".into()),
            app_id: Some("app".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .unwrap();

        let ai = get_ai(
            Some(app),
            Some(AiOptions {
                backend: Some(Backend::google_ai()),
                use_limited_use_app_check_tokens: Some(true),
            }),
        )
        .await
        .unwrap();

        ai.set_transport_for_tests(Arc::new(transport.clone()));
        ai.override_tokens_for_tests(
            None,
            Some("standard-token".into()),
            Some("limited-token".into()),
        );

        let response = ai
            .generate_text(GenerateTextRequest {
                prompt: "token test".to_string(),
                model: Some("models/gemini-pro".to_string()),
                request_options: None,
            })
            .await
            .unwrap();

        assert_eq!(response.text, "Limited token response");

        let requests = transport.take_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].header("x-firebase-appcheck"),
            Some("limited-token")
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn empty_prompt_errors() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            api_key: Some("api".into()),
            app_id: Some("app".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .unwrap();
        let ai = get_ai_service(Some(app)).await.unwrap();
        let err = ai
            .generate_text(GenerateTextRequest {
                prompt: "  ".to_string(),
                model: None,
                request_options: None,
            })
            .await
            .unwrap_err();
        assert_eq!(err.code_str(), "AI/invalid-argument");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn backend_identifier_creates_unique_instances() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .unwrap();

        let google = get_ai(
            Some(app.clone()),
            Some(AiOptions {
                backend: Some(Backend::google_ai()),
                use_limited_use_app_check_tokens: None,
            }),
        )
        .await
        .unwrap();

        let vertex = get_ai(
            Some(app.clone()),
            Some(AiOptions {
                backend: Some(Backend::vertex_ai("europe-west4")),
                use_limited_use_app_check_tokens: Some(true),
            }),
        )
        .await
        .unwrap();

        assert_ne!(Arc::as_ptr(&google), Arc::as_ptr(&vertex));
        assert_eq!(vertex.location(), Some("europe-west4"));
        assert!(vertex.options().use_limited_use_app_check_tokens);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn get_ai_reuses_cached_instance() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            api_key: Some("api".into()),
            app_id: Some("app".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .unwrap();

        let first = get_ai_service(Some(app.clone())).await.unwrap();
        first
            .prepare_generate_content_request("models/test-model", json!({ "contents": [] }), None)
            .await
            .unwrap();

        let second = get_ai(Some(app.clone()), None).await.unwrap();
        assert_eq!(Arc::as_ptr(&first), Arc::as_ptr(&second));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn api_settings_require_project_id() {
        let options = FirebaseOptions {
            api_key: Some("api".into()),
            app_id: Some("app".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .unwrap();
        let ai = get_ai_service(Some(app)).await.unwrap();
        let err = ai.api_settings().await.unwrap_err();
        assert_eq!(err.code(), AiErrorCode::NoProjectId);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn prepare_generate_content_request_builds_expected_url() {
        let options = FirebaseOptions {
            api_key: Some("api".into()),
            project_id: Some("project".into()),
            app_id: Some("app".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .unwrap();
        let ai = get_ai_service(Some(app)).await.unwrap();
        let prepared = ai
            .prepare_generate_content_request(
                "models/gemini-1.5-flash",
                json!({ "contents": [] }),
                None,
            )
            .await
            .unwrap();
        assert_eq!(
            prepared.url.as_str(),
            "https://firebasevertexai.googleapis.com/v1beta/projects/project/models/gemini-1.5-flash:generateContent"
        );
        assert_eq!(prepared.header("x-goog-api-key"), Some("api"));
    }
}
