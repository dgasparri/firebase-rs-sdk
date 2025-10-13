use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use serde_json::{json, Value};

use crate::app;
use crate::app::FirebaseApp;
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType};
use crate::data_connect::constants::DATA_CONNECT_COMPONENT_NAME;
use crate::data_connect::error::{internal_error, invalid_argument, DataConnectResult};

#[derive(Clone, Debug)]
pub struct DataConnectService {
    inner: Arc<DataConnectInner>,
}

#[derive(Debug)]
struct DataConnectInner {
    app: FirebaseApp,
    endpoint: Option<String>,
}

static DATA_CONNECT_CACHE: LazyLock<
    Mutex<HashMap<(String, Option<String>), Arc<DataConnectService>>>,
> = LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Debug, PartialEq)]
pub struct QueryRequest {
    pub operation: String,
    pub variables: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QueryResponse {
    pub data: Value,
}

impl DataConnectService {
    fn new(app: FirebaseApp, endpoint: Option<String>) -> Self {
        Self {
            inner: Arc::new(DataConnectInner { app, endpoint }),
        }
    }

    pub fn app(&self) -> &FirebaseApp {
        &self.inner.app
    }

    pub fn endpoint(&self) -> Option<&str> {
        self.inner.endpoint.as_deref()
    }

    pub fn execute(&self, request: QueryRequest) -> DataConnectResult<QueryResponse> {
        if request.operation.trim().is_empty() {
            return Err(invalid_argument("Operation text must not be empty"));
        }
        let payload = json!({
            "operation": request.operation,
            "variables": request.variables,
            "endpoint": self.endpoint().unwrap_or("default"),
        });
        Ok(QueryResponse { data: payload })
    }
}

static DATA_CONNECT_COMPONENT: LazyLock<()> = LazyLock::new(|| {
    let component = Component::new(
        DATA_CONNECT_COMPONENT_NAME,
        Arc::new(data_connect_factory),
        ComponentType::Public,
    )
    .with_instantiation_mode(InstantiationMode::Lazy)
    .with_multiple_instances(true);
    let _ = app::registry::register_component(component);
});

fn data_connect_factory(
    container: &crate::component::ComponentContainer,
    options: InstanceFactoryOptions,
) -> Result<DynService, ComponentError> {
    let app = container.root_service::<FirebaseApp>().ok_or_else(|| {
        ComponentError::InitializationFailed {
            name: DATA_CONNECT_COMPONENT_NAME.to_string(),
            reason: "Firebase app not attached to component container".to_string(),
        }
    })?;

    let endpoint = options
        .options
        .get("endpoint")
        .and_then(|value| value.as_str().map(|s| s.to_string()))
        .or(options.instance_identifier.clone());

    let service = DataConnectService::new((*app).clone(), endpoint);
    Ok(Arc::new(service) as DynService)
}

fn ensure_registered() {
    LazyLock::force(&DATA_CONNECT_COMPONENT);
}

pub fn register_data_connect_component() {
    ensure_registered();
}

pub fn get_data_connect_service(
    app: Option<FirebaseApp>,
    endpoint: Option<&str>,
) -> DataConnectResult<Arc<DataConnectService>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => crate::app::api::get_app(None).map_err(|err| internal_error(err.to_string()))?,
    };

    let endpoint_string = endpoint.map(|e| e.to_string());
    let cache_key = (app.name().to_string(), endpoint_string.clone());
    if let Some(service) = DATA_CONNECT_CACHE.lock().unwrap().get(&cache_key).cloned() {
        return Ok(service);
    }

    let provider = app::registry::get_provider(&app, DATA_CONNECT_COMPONENT_NAME);
    if let Some(service) = match endpoint {
        Some(id) => provider
            .get_immediate_with_options::<DataConnectService>(Some(id), true)
            .unwrap_or(None),
        None => provider.get_immediate::<DataConnectService>(),
    } {
        DATA_CONNECT_CACHE
            .lock()
            .unwrap()
            .insert(cache_key.clone(), service.clone());
        return Ok(service);
    }

    let options = if let Some(ref endpoint) = endpoint_string {
        json!({ "endpoint": endpoint })
    } else {
        Value::Null
    };

    match provider.initialize::<DataConnectService>(options, endpoint) {
        Ok(service) => {
            DATA_CONNECT_CACHE
                .lock()
                .unwrap()
                .insert(cache_key, service.clone());
            Ok(service)
        }
        Err(crate::component::types::ComponentError::InstanceUnavailable { .. }) => {
            if let Some(service) = match endpoint {
                Some(id) => provider
                    .get_immediate_with_options::<DataConnectService>(Some(id), true)
                    .unwrap_or(None),
                None => provider.get_immediate::<DataConnectService>(),
            } {
                DATA_CONNECT_CACHE
                    .lock()
                    .unwrap()
                    .insert(cache_key, service.clone());
                Ok(service)
            } else {
                let fallback = Arc::new(DataConnectService::new(
                    app.clone(),
                    endpoint_string.clone(),
                ));
                DATA_CONNECT_CACHE
                    .lock()
                    .unwrap()
                    .insert(cache_key, fallback.clone());
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
            name: Some(format!(
                "data-connect-{}",
                COUNTER.fetch_add(1, Ordering::SeqCst)
            )),
            ..Default::default()
        }
    }

    #[test]
    fn execute_returns_stub_payload() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let service =
            get_data_connect_service(Some(app), Some("https://example/graphql")).expect("service");
        let mut vars = BTreeMap::new();
        vars.insert("id".into(), json!(123));
        let response = service
            .execute(QueryRequest {
                operation: "query GetItem { item { id } }".into(),
                variables: vars.clone(),
            })
            .unwrap();
        assert!(response
            .data
            .get("operation")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("GetItem"));
        assert_eq!(response.data.get("variables").unwrap(), &json!(vars));
    }

    #[test]
    fn empty_operation_errors() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let service = get_data_connect_service(Some(app), None).unwrap();
        let err = service
            .execute(QueryRequest {
                operation: "   ".into(),
                variables: BTreeMap::new(),
            })
            .unwrap_err();
        assert_eq!(err.code_str(), "data-connect/invalid-argument");
    }
}
