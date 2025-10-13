use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use crate::ai::constants::AI_COMPONENT_NAME;
use crate::ai::error::{internal_error, invalid_argument, AiResult};
use crate::app;
use crate::app::FirebaseApp;
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
    default_model: Option<String>,
}

static AI_OVERRIDES: LazyLock<Mutex<HashMap<String, Arc<AiService>>>> =
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
    fn new(app: FirebaseApp, default_model: Option<String>) -> Self {
        Self {
            inner: Arc::new(AiInner { app, default_model }),
        }
    }

    pub fn app(&self) -> &FirebaseApp {
        &self.inner.app
    }

    pub fn generate_text(&self, request: GenerateTextRequest) -> AiResult<GenerateTextResponse> {
        if request.prompt.trim().is_empty() {
            return Err(invalid_argument("Prompt must not be empty"));
        }
        let model = request
            .model
            .or_else(|| self.inner.default_model.clone())
            .unwrap_or_else(|| "text-bison-001".to_string());
        // Minimal stub: echo back the prompt length info.
        let synthetic = format!("[model:{}] generated {} chars", model, request.prompt.len());
        Ok(GenerateTextResponse {
            text: synthetic,
            model,
        })
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

    let default_model = options
        .options
        .get("defaultModel")
        .and_then(|value| value.as_str().map(|s| s.to_string()));
    let service = AiService::new((*app).clone(), default_model);
    Ok(Arc::new(service) as DynService)
}

fn ensure_registered() {
    LazyLock::force(&AI_COMPONENT);
}

pub fn register_ai_component() {
    ensure_registered();
}

pub fn get_ai_service(app: Option<FirebaseApp>) -> AiResult<Arc<AiService>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => crate::app::api::get_app(None).map_err(|err| internal_error(err.to_string()))?,
    };

    if let Some(service) = AI_OVERRIDES.lock().unwrap().get(app.name()).cloned() {
        return Ok(service);
    }

    let provider = app::registry::get_provider(&app, AI_COMPONENT_NAME);
    if let Some(service) = provider.get_immediate::<AiService>() {
        AI_OVERRIDES
            .lock()
            .unwrap()
            .insert(app.name().to_string(), service.clone());
        return Ok(service);
    }

    match provider.initialize::<AiService>(serde_json::Value::Null, None) {
        Ok(service) => {
            AI_OVERRIDES
                .lock()
                .unwrap()
                .insert(app.name().to_string(), service.clone());
            Ok(service)
        }
        Err(crate::component::types::ComponentError::InstanceUnavailable { .. }) => {
            if let Some(service) = provider.get_immediate::<AiService>() {
                AI_OVERRIDES
                    .lock()
                    .unwrap()
                    .insert(app.name().to_string(), service.clone());
                Ok(service)
            } else {
                let fallback = Arc::new(AiService::new(app.clone(), None));
                AI_OVERRIDES
                    .lock()
                    .unwrap()
                    .insert(app.name().to_string(), fallback.clone());
                Ok(fallback)
            }
        }
        Err(err) => Err(internal_error(err.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::api::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};

    fn unique_settings() -> FirebaseAppSettings {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!("ai-{}", COUNTER.fetch_add(1, Ordering::SeqCst))),
            ..Default::default()
        }
    }

    #[test]
    fn generate_text_uses_prompt_length() {
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
    }

    #[test]
    fn empty_prompt_errors() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
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
        assert_eq!(err.code_str(), "ai/invalid-argument");
    }
}
