use std::sync::Arc;

use serde_json::Value;

use crate::ai::api::AiService;
use crate::ai::backend::BackendType;
use crate::ai::error::AiResult;
use crate::ai::requests::{PreparedRequest, RequestOptions, Task};

/// Port of the Firebase JS SDK `GenerativeModel` class.
///
/// Reference: `packages/ai/src/models/generative-model.ts`.
#[derive(Clone, Debug)]
pub struct GenerativeModel {
    service: Arc<AiService>,
    model: String,
    default_request_options: Option<RequestOptions>,
}

impl GenerativeModel {
    /// Creates a new generative model bound to the provided `AiService`.
    ///
    /// This mirrors the TypeScript constructor, normalising the model name according to the selected
    /// backend and capturing the service API settings for later requests.
    pub fn new(
        service: Arc<AiService>,
        model_name: impl Into<String>,
        request_options: Option<RequestOptions>,
    ) -> AiResult<Self> {
        let backend_type = service.backend_type();
        let model = normalize_model_name(model_name.into(), backend_type);
        Ok(Self {
            service,
            model,
            default_request_options: request_options,
        })
    }

    /// Returns the fully qualified model resource identifier.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Prepares a `generateContent` request using the stored API settings.
    pub async fn prepare_generate_content_request(
        &self,
        body: Value,
        request_options: Option<RequestOptions>,
    ) -> AiResult<PreparedRequest> {
        let factory = self.service.request_factory().await?;
        let effective_options = request_options.or_else(|| self.default_request_options.clone());
        factory.construct_request(&self.model, Task::GenerateContent, false, body, effective_options)
    }
}

fn normalize_model_name(model: String, backend_type: BackendType) -> String {
    match backend_type {
        BackendType::GoogleAi => normalize_google_ai_model(model),
        BackendType::VertexAi => normalize_vertex_ai_model(model),
    }
}

fn normalize_google_ai_model(model: String) -> String {
    if model.starts_with("models/") {
        model
    } else {
        format!("models/{model}")
    }
}

fn normalize_vertex_ai_model(model: String) -> String {
    if model.contains('/') {
        if model.starts_with("models/") {
            format!("publishers/google/{model}")
        } else {
            model
        }
    } else {
        format!("publishers/google/models/{model}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::backend::Backend;
    use crate::ai::public_types::AiOptions;
    use crate::app::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use serde_json::json;
    use std::time::Duration;

    fn unique_settings() -> FirebaseAppSettings {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!("gen-model-{}", COUNTER.fetch_add(1, Ordering::SeqCst))),
            ..Default::default()
        }
    }

    async fn init_service(options: FirebaseOptions, backend: Option<Backend>) -> Arc<AiService> {
        let app = initialize_app(options, Some(unique_settings())).await.unwrap();
        match backend {
            Some(backend) => crate::ai::get_ai(
                Some(app),
                Some(AiOptions {
                    backend: Some(backend),
                    use_limited_use_app_check_tokens: None,
                }),
            )
            .await
            .unwrap(),
            None => crate::ai::get_ai_service(Some(app)).await.unwrap(),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn normalizes_google_model_name() {
        let service = init_service(
            FirebaseOptions {
                api_key: Some("api".into()),
                project_id: Some("project".into()),
                app_id: Some("app".into()),
                ..Default::default()
            },
            None,
        )
        .await;
        let model = GenerativeModel::new(service.clone(), "gemini-pro", None).unwrap();
        assert_eq!(model.model(), "models/gemini-pro");

        let already_prefixed = GenerativeModel::new(service, "models/gemini-pro", None).unwrap();
        assert_eq!(already_prefixed.model(), "models/gemini-pro");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn normalizes_vertex_model_name_and_prepares_request() {
        let service = init_service(
            FirebaseOptions {
                api_key: Some("api".into()),
                project_id: Some("project".into()),
                app_id: Some("app".into()),
                ..Default::default()
            },
            Some(Backend::vertex_ai("us-central1")),
        )
        .await;
        let model = GenerativeModel::new(
            service,
            "gemini-pro",
            Some(RequestOptions {
                timeout: Some(Duration::from_secs(5)),
                base_url: Some("https://example.com".into()),
            }),
        )
        .unwrap();

        assert_eq!(model.model(), "publishers/google/models/gemini-pro");

        let prepared = model
            .prepare_generate_content_request(json!({"contents": []}), None)
            .await
            .unwrap();
        assert_eq!(
            prepared.url.as_str(),
            "https://example.com/v1beta/projects/project/locations/us-central1/publishers/google/models/gemini-pro:generateContent"
        );
        assert_eq!(prepared.timeout, Duration::from_secs(5));
    }
}
