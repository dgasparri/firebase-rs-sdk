use std::sync::{Arc, LazyLock};

use crate::app;
use crate::app::FirebaseApp;
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType};
use crate::functions::constants::FUNCTIONS_COMPONENT_NAME;
use crate::functions::error::{internal_error, invalid_argument, FunctionsResult};

#[derive(Clone, Debug)]
pub struct Functions {
    inner: Arc<FunctionsInner>,
}

#[derive(Debug)]
struct FunctionsInner {
    app: FirebaseApp,
    region: String,
}

impl Functions {
    fn new(app: FirebaseApp, region: String) -> Self {
        Self {
            inner: Arc::new(FunctionsInner { app, region }),
        }
    }

    pub fn app(&self) -> &FirebaseApp {
        &self.inner.app
    }

    pub fn region(&self) -> &str {
        &self.inner.region
    }

    pub fn https_callable<T>(&self, name: &str) -> FunctionsResult<CallableFunction<T>>
    where
        T: serde::de::DeserializeOwned + serde::Serialize + 'static,
    {
        if name.trim().is_empty() {
            return Err(invalid_argument("Function name must not be empty"));
        }
        Ok(CallableFunction {
            functions: self.clone(),
            name: name.to_string(),
            _marker: std::marker::PhantomData,
        })
    }
}

#[derive(Clone)]
pub struct CallableFunction<T> {
    functions: Functions,
    name: String,
    _marker: std::marker::PhantomData<T>,
}

impl<T> CallableFunction<T>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    pub fn call(&self, data: &T) -> FunctionsResult<T> {
        // Minimal stub: simply serialize/deserialize to simulate a roundtrip.
        let json = serde_json::to_value(data).map_err(|err| internal_error(err.to_string()))?;
        serde_json::from_value(json).map_err(|err| internal_error(err.to_string()))
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn region(&self) -> &str {
        self.functions.region()
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
    let _ = app::registry::register_component(component);
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

    let region = options
        .instance_identifier
        .clone()
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| "us-central1".to_string());

    let functions = Functions::new((*app).clone(), region);
    Ok(Arc::new(functions) as DynService)
}

fn ensure_registered() {
    LazyLock::force(&FUNCTIONS_COMPONENT);
}

pub fn register_functions_component() {
    ensure_registered();
}

pub fn get_functions(
    app: Option<FirebaseApp>,
    region: Option<&str>,
) -> FunctionsResult<Arc<Functions>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => crate::app::api::get_app(None).map_err(|err| internal_error(err.to_string()))?,
    };

    let provider = app::registry::get_provider(&app, FUNCTIONS_COMPONENT_NAME);
    if let Some(region) = region {
        provider
            .initialize::<Functions>(serde_json::Value::Null, Some(region))
            .map_err(|err| internal_error(err.to_string()))
    } else {
        provider
            .get_immediate::<Functions>()
            .ok_or_else(|| internal_error("Functions component not available"))
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
                "functions-{}",
                COUNTER.fetch_add(1, Ordering::SeqCst)
            )),
            ..Default::default()
        }
    }

    #[test]
    fn https_callable_roundtrip() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let functions = get_functions(Some(app), None).unwrap();
        let callable = functions
            .https_callable::<serde_json::Value>("hello")
            .unwrap();
        let payload = serde_json::json!({ "message": "ping" });
        let response = callable.call(&payload).unwrap();
        assert_eq!(response, payload);
    }
}
