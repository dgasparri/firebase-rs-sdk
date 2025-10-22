use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use serde_json::{json, Value};

use crate::ai::backend::{Backend, BackendType};
use crate::ai::constants::AI_COMPONENT_NAME;
use crate::ai::error::{internal_error, invalid_argument, AiError, AiErrorCode, AiResult};
use crate::ai::helpers::{decode_instance_identifier, encode_instance_identifier};
use crate::ai::public_types::{AiOptions, AiRuntimeOptions};
use crate::ai::requests::{ApiSettings, PreparedRequest, RequestFactory, RequestOptions, Task};
use crate::app;
use crate::app::{FirebaseApp, FirebaseOptions};
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType};

#[derive(Clone, Debug)]
pub struct AiService {
    inner: Arc<AiInner>,
}

#[derive(Debug)]
struct AiInner {
    app: FirebaseApp,
    backend: Backend,
    options: Mutex<AiRuntimeOptions>,
    default_model: Option<String>,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerateTextRequest {
    pub prompt: String,
    pub model: Option<String>,
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
    ) -> Self {
        Self {
            inner: Arc::new(AiInner {
                app,
                backend,
                options: Mutex::new(options),
                default_model,
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

    pub(crate) fn api_settings(&self) -> AiResult<ApiSettings> {
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

        let automatic = self.inner.app.automatic_data_collection_enabled();
        Ok(ApiSettings::new(
            api_key,
            project_id,
            app_id,
            self.inner.backend.clone(),
            automatic,
            None,
            None,
        ))
    }

    pub(crate) fn request_factory(&self) -> AiResult<RequestFactory> {
        Ok(RequestFactory::new(self.api_settings()?))
    }

    /// Prepares a REST request for a `generateContent` call without executing it.
    ///
    /// This mirrors the behaviour of `constructRequest` in the TypeScript SDK and allows advanced
    /// callers to integrate with custom HTTP stacks while the SDK handles URL/header generation.
    pub fn prepare_generate_content_request(
        &self,
        model: &str,
        body: Value,
        request_options: Option<RequestOptions>,
    ) -> AiResult<PreparedRequest> {
        let factory = self.request_factory()?;
        factory.construct_request(model, Task::GenerateContent, false, body, request_options)
    }

    pub fn generate_text(&self, request: GenerateTextRequest) -> AiResult<GenerateTextResponse> {
        if request.prompt.trim().is_empty() {
            return Err(invalid_argument("Prompt must not be empty"));
        }
        let model = request
            .model
            .or_else(|| self.inner.default_model.clone())
            .unwrap_or_else(|| "text-bison-001".to_string());

        let backend_label = self.backend_type().to_string();
        let location_suffix = self
            .location()
            .map(|loc| format!(" @{}", loc))
            .unwrap_or_default();
        let synthetic = format!(
            "[backend:{}{}] generated {} chars",
            backend_label,
            location_suffix,
            request.prompt.len()
        );
        Ok(GenerateTextResponse {
            text: synthetic,
            model,
        })
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
    let _ = app::registry::register_component(component);
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

    let service = AiService::new((*app).clone(), backend, runtime_options, default_model);
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
/// # use firebase_rs_sdk_unofficial::ai::backend::Backend;
/// # use firebase_rs_sdk_unofficial::ai::public_types::AiOptions;
/// # use firebase_rs_sdk_unofficial::ai::get_ai;
/// # use firebase_rs_sdk_unofficial::app::api::initialize_app;
/// # use firebase_rs_sdk_unofficial::app::{FirebaseAppSettings, FirebaseOptions};
/// let options = FirebaseOptions {
///     project_id: Some("project".into()),
///     api_key: Some("test".into()),
///     ..Default::default()
/// };
/// let app = initialize_app(options, Some(FirebaseAppSettings::default())).unwrap();
/// let ai = get_ai(Some(app), Some(AiOptions {
///     backend: Some(Backend::vertex_ai("us-central1")),
///     use_limited_use_app_check_tokens: Some(false),
/// }));
/// assert!(ai.is_ok());
/// ```
pub fn get_ai(app: Option<FirebaseApp>, options: Option<AiOptions>) -> AiResult<Arc<AiService>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => crate::app::api::get_app(None).map_err(|err| internal_error(err.to_string()))?,
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

    let provider = app::registry::get_provider(&app, AI_COMPONENT_NAME);

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
                let fallback =
                    Arc::new(AiService::new(app.clone(), backend, runtime_options, None));
                Cache::insert(cache_key.clone(), fallback.clone());
                Ok(fallback)
            }
        }
        Err(err) => Err(internal_error(err.to_string())),
    }
}

/// Convenience wrapper that mirrors the original Rust stub signature.
pub fn get_ai_service(app: Option<FirebaseApp>) -> AiResult<Arc<AiService>> {
    get_ai(app, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::backend::Backend;
    use crate::ai::error::AiErrorCode;
    use crate::ai::public_types::AiOptions;
    use crate::app::api::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use serde_json::json;

    fn unique_settings() -> FirebaseAppSettings {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!("ai-{}", COUNTER.fetch_add(1, Ordering::SeqCst))),
            ..Default::default()
        }
    }

    #[test]
    fn generate_text_includes_backend_info() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let ai = get_ai_service(Some(app)).unwrap();
        let response = ai
            .generate_text(GenerateTextRequest {
                prompt: "Hello AI".to_string(),
                model: Some("text-test".to_string()),
            })
            .unwrap();
        assert_eq!(response.model, "text-test");
        assert!(response.text.contains("generated 8 chars"));
        assert!(response.text.contains("backend:GOOGLE_AI"));
    }

    #[test]
    fn empty_prompt_errors() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            api_key: Some("api".into()),
            app_id: Some("app".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let ai = get_ai_service(Some(app)).unwrap();
        let err = ai
            .generate_text(GenerateTextRequest {
                prompt: "  ".to_string(),
                model: None,
            })
            .unwrap_err();
        assert_eq!(err.code_str(), "AI/invalid-argument");
    }

    #[test]
    fn backend_identifier_creates_unique_instances() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();

        let google = get_ai(
            Some(app.clone()),
            Some(AiOptions {
                backend: Some(Backend::google_ai()),
                use_limited_use_app_check_tokens: None,
            }),
        )
        .unwrap();

        let vertex = get_ai(
            Some(app.clone()),
            Some(AiOptions {
                backend: Some(Backend::vertex_ai("europe-west4")),
                use_limited_use_app_check_tokens: Some(true),
            }),
        )
        .unwrap();

        assert_ne!(Arc::as_ptr(&google), Arc::as_ptr(&vertex));
        assert_eq!(vertex.location(), Some("europe-west4"));
        assert!(vertex.options().use_limited_use_app_check_tokens);
    }

    #[test]
    fn get_ai_reuses_cached_instance() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            api_key: Some("api".into()),
            app_id: Some("app".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();

        let first = get_ai_service(Some(app.clone())).unwrap();
        first
            .generate_text(GenerateTextRequest {
                prompt: "ping".to_string(),
                model: None,
            })
            .unwrap();

        let second = get_ai(Some(app.clone()), None).unwrap();
        assert_eq!(Arc::as_ptr(&first), Arc::as_ptr(&second));
    }

    #[test]
    fn api_settings_require_project_id() {
        let options = FirebaseOptions {
            api_key: Some("api".into()),
            app_id: Some("app".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let ai = get_ai_service(Some(app)).unwrap();
        let err = ai.api_settings().unwrap_err();
        assert_eq!(err.code(), AiErrorCode::NoProjectId);
    }

    #[test]
    fn prepare_generate_content_request_builds_expected_url() {
        let options = FirebaseOptions {
            api_key: Some("api".into()),
            project_id: Some("project".into()),
            app_id: Some("app".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let ai = get_ai_service(Some(app)).unwrap();
        let prepared = ai
            .prepare_generate_content_request(
                "models/gemini-1.5-flash",
                json!({ "contents": [] }),
                None,
            )
            .unwrap();
        assert_eq!(
            prepared.url.as_str(),
            "https://firebasevertexai.googleapis.com/v1beta/projects/project/models/gemini-1.5-flash:generateContent"
        );
        assert_eq!(prepared.header("x-goog-api-key"), Some("api"));
    }
}
