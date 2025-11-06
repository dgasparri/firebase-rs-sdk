use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use crate::app;
use crate::app::FirebaseApp;
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType};
use crate::functions::constants::FUNCTIONS_COMPONENT_NAME;
use crate::functions::context::ContextProvider;
use crate::functions::error::{internal_error, invalid_argument, FunctionsResult};
use crate::functions::transport::{invoke_callable_async, CallableRequest};
use serde_json::{json, Value as JsonValue};
use url::Url;

#[cfg(test)]
use crate::functions::context::CallContext;

const DEFAULT_REGION: &str = "us-central1";
const DEFAULT_TIMEOUT_MS: u64 = 70_000;

/// Client entry point for invoking HTTPS callable Cloud Functions.
///
/// This mirrors the JavaScript `FunctionsService` implementation in
/// [`packages/functions/src/service.ts`](../../packages/functions/src/service.ts), exposing
/// a strongly-typed Rust surface that aligns with the modular SDK.
#[derive(Clone, Debug)]
pub struct Functions {
    inner: Arc<FunctionsInner>,
}

#[derive(Debug)]
struct FunctionsInner {
    app: FirebaseApp,
    endpoint: Endpoint,
    context: ContextProvider,
}

impl Functions {
    fn new(app: FirebaseApp, endpoint: Endpoint) -> Self {
        let context = ContextProvider::new(app.clone());
        Self {
            inner: Arc::new(FunctionsInner {
                app,
                endpoint,
                context,
            }),
        }
    }

    pub fn app(&self) -> &FirebaseApp {
        &self.inner.app
    }

    pub fn region(&self) -> &str {
        self.inner.endpoint.region()
    }

    /// Returns a typed callable reference for the given Cloud Function name.
    ///
    /// This is the Rust equivalent of
    /// [`httpsCallable`](https://firebase.google.com/docs/functions/callable-reference) from the
    /// JavaScript SDK (`packages/functions/src/service.ts`).
    ///
    /// # Examples
    /// ```ignore
    /// # use firebase_rs_sdk::functions::{get_functions, register_functions_component};
    /// # use firebase_rs_sdk::functions::error::FunctionsResult;
    /// # use firebase_rs_sdk::app::initialize_app;
    /// # use firebase_rs_sdk::app::{FirebaseAppSettings, FirebaseOptions};
    /// # async fn demo() -> firebase_rs_sdk::functions::error::FunctionsResult<()> {
    /// # register_functions_component();
    /// # let app = initialize_app(FirebaseOptions {
    /// #     project_id: Some("demo-project".into()),
    /// #     ..Default::default()
    /// # }, Some(FirebaseAppSettings::default())).await.unwrap();
    /// # use serde_json::json;
    /// let functions = get_functions(Some(app.clone()), None).await?;
    /// let callable = functions
    ///     .https_callable::<serde_json::Value, serde_json::Value>("helloWorld")?;
    /// let response = callable
    ///     .call_async(&json!({"text": "hi"}))
    ///     .await?;
    /// println!("{:?}", response);
    /// # Ok(())
    /// # }
    /// # let _ = demo;
    /// ```
    pub fn https_callable<Request, Response>(
        &self,
        name: &str,
    ) -> FunctionsResult<CallableFunction<Request, Response>>
    where
        Request: serde::Serialize + 'static,
        Response: serde::de::DeserializeOwned + 'static,
    {
        if name.trim().is_empty() {
            return Err(invalid_argument("Function name must not be empty"));
        }
        Ok(CallableFunction {
            functions: self.clone(),
            name: name.trim().trim_matches('/').to_string(),
            _request: std::marker::PhantomData,
            _response: std::marker::PhantomData,
        })
    }

    fn callable_url(&self, name: &str) -> FunctionsResult<String> {
        let sanitized = name.trim_start_matches('/');
        let options = self.inner.app.options();
        let project_id = options.project_id.ok_or_else(|| {
            invalid_argument("FirebaseOptions.project_id is required to call Functions")
        })?;
        self.inner.endpoint.callable_url(&project_id, sanitized)
    }

    fn context(&self) -> &ContextProvider {
        &self.inner.context
    }

    #[cfg(test)]
    pub fn set_context_overrides(&self, overrides: CallContext) {
        self.inner.context.set_overrides(overrides);
    }
}

/// Callable Cloud Function handle that can be invoked with typed payloads.
///
/// The shape follows the JavaScript `HttpsCallable` returned from
/// `httpsCallable()` in `packages/functions/src/service.ts`.
#[derive(Clone)]
pub struct CallableFunction<Request, Response> {
    functions: Functions,
    name: String,
    _request: std::marker::PhantomData<Request>,
    _response: std::marker::PhantomData<Response>,
}

impl<Request, Response> CallableFunction<Request, Response>
where
    Request: serde::Serialize,
    Response: serde::de::DeserializeOwned,
{
    /// Asynchronously invokes the backend function and returns the decoded response payload.
    ///
    /// The request and response serialization mirrors the JavaScript SDK behaviour: payloads are
    /// encoded as JSON objects (`{ "data": ... }`) and any server error is mapped to a
    /// `FunctionsError` code.
    ///
    /// This method is available on all targets and should be awaited within the caller's async
    /// runtime.
    pub async fn call_async(&self, data: &Request) -> FunctionsResult<Response> {
        let payload = serde_json::to_value(data).map_err(|err| {
            internal_error(format!("Failed to serialize callable payload: {err}"))
        })?;
        let body = json!({ "data": payload });
        let url = self.functions.callable_url(&self.name)?;
        let mut request =
            CallableRequest::new(url, body, Duration::from_millis(DEFAULT_TIMEOUT_MS));
        request
            .headers
            .insert("Content-Type".to_string(), "application/json".to_string());

        let context = self.functions.context().get_context_async(false).await;
        if let Some(token) = context.auth_token {
            if !token.is_empty() {
                request
                    .headers
                    .insert("Authorization".to_string(), format!("Bearer {token}"));
            }
        }
        if let Some(token) = context.messaging_token {
            if !token.is_empty() {
                request
                    .headers
                    .insert("Firebase-Instance-ID-Token".to_string(), token);
            }
        }
        if let Some(token) = context.app_check_token {
            if !token.is_empty() {
                request
                    .headers
                    .insert("X-Firebase-AppCheck".to_string(), token);
            }
        }

        if let Some(header) = context.app_check_heartbeat {
            if !header.is_empty() {
                request
                    .headers
                    .insert("X-Firebase-Client".to_string(), header);
            }
        }

        let response_body = invoke_callable_async(request).await?;
        extract_data(response_body)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn region(&self) -> &str {
        self.functions.region()
    }
}

fn extract_data<Response>(body: JsonValue) -> FunctionsResult<Response>
where
    Response: serde::de::DeserializeOwned,
{
    match body {
        JsonValue::Object(mut map) => {
            if let Some(data_value) = map.remove("data").or_else(|| map.remove("result")) {
                serde_json::from_value(data_value).map_err(|err| {
                    internal_error(format!(
                        "Failed to deserialize callable response payload: {err}"
                    ))
                })
            } else {
                Err(internal_error(
                    "Callable response JSON is missing a data field",
                ))
            }
        }
        JsonValue::Null => Err(internal_error(
            "Callable response did not contain a JSON payload",
        )),
        other => Err(internal_error(format!(
            "Unexpected callable response shape: expected object, got {other}"
        ))),
    }
}

#[derive(Clone, Debug)]
struct Endpoint {
    region: String,
    custom_domain: Option<String>,
    emulator_origin: Arc<Mutex<Option<String>>>,
}

impl Endpoint {
    fn new(identifier: Option<String>) -> Self {
        match identifier.and_then(|value| {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }) {
            Some(raw) => match Url::parse(&raw) {
                Ok(url) => {
                    let origin = url.origin().ascii_serialization();
                    let mut normalized = origin;
                    let path = url.path();
                    if path != "/" {
                        normalized.push_str(path.trim_end_matches('/'));
                    }
                    Self {
                        region: DEFAULT_REGION.to_string(),
                        custom_domain: Some(normalized),
                        emulator_origin: Arc::new(Mutex::new(None)),
                    }
                }
                Err(_) => Self {
                    region: raw,
                    custom_domain: None,
                    emulator_origin: Arc::new(Mutex::new(None)),
                },
            },
            None => Self::default(),
        }
    }

    fn region(&self) -> &str {
        &self.region
    }

    fn callable_url(&self, project_id: &str, name: &str) -> FunctionsResult<String> {
        if let Some(origin) = self.emulator_origin.lock().unwrap().clone() {
            return Ok(format!("{origin}/{project_id}/{}/{}", self.region, name));
        }

        if let Some(domain) = &self.custom_domain {
            return Ok(format!("{}/{}", domain.trim_end_matches('/'), name));
        }

        Ok(format!(
            "https://{}-{}.cloudfunctions.net/{}",
            self.region, project_id, name
        ))
    }
}

impl Default for Endpoint {
    fn default() -> Self {
        Self {
            region: DEFAULT_REGION.to_string(),
            custom_domain: None,
            emulator_origin: Arc::new(Mutex::new(None)),
        }
    }
}

static FUNCTIONS_COMPONENT: LazyLock<()> = LazyLock::new(|| {
    let component = Component::new(
        FUNCTIONS_COMPONENT_NAME,
        Arc::new(functions_factory),
        ComponentType::Public,
    )
    .with_instantiation_mode(InstantiationMode::Lazy)
    .with_multiple_instances(true);
    let _ = app::register_component(component);
});

fn functions_factory(
    container: &crate::component::ComponentContainer,
    options: InstanceFactoryOptions,
) -> Result<DynService, ComponentError> {
    let app = container.root_service::<FirebaseApp>().ok_or_else(|| {
        ComponentError::InitializationFailed {
            name: FUNCTIONS_COMPONENT_NAME.to_string(),
            reason: "Firebase app not attached to component container".to_string(),
        }
    })?;

    let endpoint = Endpoint::new(options.instance_identifier.clone());
    let functions = Functions::new((*app).clone(), endpoint);
    Ok(Arc::new(functions) as DynService)
}

fn ensure_registered() {
    LazyLock::force(&FUNCTIONS_COMPONENT);
}

/// Registers the Functions component with the global app container.
///
/// Equivalent to the JavaScript bootstrap performed in
/// `packages/functions/src/register.ts`. Call this before resolving the service manually.
pub fn register_functions_component() {
    ensure_registered();
}

/// Fetches (or lazily creates) a `Functions` client for the given Firebase app.
///
/// Mirrors the modular helper exported from `packages/functions/src/api.ts`.
/// Passing `region_or_domain` allows selecting a different region or custom domain just like the
/// `app.functions('europe-west1')` overload in the JavaScript SDK.
pub async fn get_functions(
    app: Option<FirebaseApp>,
    region_or_domain: Option<&str>,
) -> FunctionsResult<Arc<Functions>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => crate::app::get_app(None)
            .await
            .map_err(|err| internal_error(err.to_string()))?,
    };

    let provider = app::get_provider(&app, FUNCTIONS_COMPONENT_NAME);
    if let Some(identifier) = region_or_domain {
        provider
            .initialize::<Functions>(serde_json::Value::Null, Some(identifier))
            .map_err(|err| internal_error(err.to_string()))
    } else {
        provider
            .get_immediate::<Functions>()
            .ok_or_else(|| internal_error("Functions component not available"))
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::app::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use httpmock::Method::POST;
    use httpmock::MockServer;
    use std::panic;

    fn unique_settings() -> FirebaseAppSettings {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!(
                "functions-{}",
                COUNTER.fetch_add(1, Ordering::SeqCst)
            )),
            ..Default::default()
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn https_callable_invokes_backend() {
        let server = match panic::catch_unwind(|| MockServer::start()) {
            Ok(server) => server,
            Err(_) => {
                eprintln!(
                    "Skipping https_callable_invokes_backend: unable to bind mock server in this environment"
                );
                return;
            }
        };
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/callable/hello")
                .json_body(json!({ "data": { "message": "ping" } }));
            then.status(200)
                .json_body(json!({ "data": { "message": "pong" } }));
        });

        let options = FirebaseOptions {
            project_id: Some("demo-project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .unwrap();
        let functions = get_functions(Some(app), Some(&server.url("/callable")))
            .await
            .unwrap();
        let callable = functions
            .https_callable::<serde_json::Value, serde_json::Value>("hello")
            .unwrap();

        let payload = json!({ "message": "ping" });
        let response = callable.call_async(&payload).await.unwrap();

        assert_eq!(response, json!({ "message": "pong" }));
        mock.assert();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn https_callable_includes_context_headers() {
        let server = match panic::catch_unwind(|| MockServer::start()) {
            Ok(server) => server,
            Err(_) => {
                eprintln!(
                    "Skipping https_callable_includes_context_headers: unable to bind mock server"
                );
                return;
            }
        };

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/callable/secureCall")
                .header("authorization", "Bearer auth-token")
                .header("firebase-instance-id-token", "iid-token")
                .header("x-firebase-appcheck", "app-check-token")
                .json_body(json!({ "data": { "ping": true } }));
            then.status(200)
                .json_body(json!({ "data": { "ok": true } }));
        });

        let options = FirebaseOptions {
            project_id: Some("demo-project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .unwrap();
        let functions = get_functions(Some(app), Some(&server.url("/callable")))
            .await
            .unwrap();
        functions.set_context_overrides(CallContext {
            auth_token: Some("auth-token".into()),
            messaging_token: Some("iid-token".into()),
            app_check_token: Some("app-check-token".into()),
            app_check_heartbeat: None,
        });

        let callable = functions
            .https_callable::<serde_json::Value, serde_json::Value>("secureCall")
            .unwrap();

        let payload = json!({ "ping": true });
        let response = callable.call_async(&payload).await.unwrap();

        assert_eq!(response, json!({ "ok": true }));
        mock.assert();
    }
}
