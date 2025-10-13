use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::app;
use crate::app::FirebaseApp;
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType};
use crate::performance::constants::PERFORMANCE_COMPONENT_NAME;
use crate::performance::error::{internal_error, invalid_argument, PerformanceResult};

#[derive(Clone, Debug)]
pub struct Performance {
    inner: Arc<PerformanceInner>,
}

#[derive(Debug)]
struct PerformanceInner {
    app: FirebaseApp,
    traces: Mutex<HashMap<String, PerformanceTrace>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PerformanceTrace {
    pub name: String,
    pub duration: Duration,
    pub metrics: HashMap<String, i64>,
}

#[derive(Clone, Debug)]
pub struct TraceHandle {
    performance: Performance,
    name: String,
    start: Instant,
    metrics: HashMap<String, i64>,
}

impl Performance {
    fn new(app: FirebaseApp) -> Self {
        Self {
            inner: Arc::new(PerformanceInner {
                app,
                traces: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn app(&self) -> &FirebaseApp {
        &self.inner.app
    }

    pub fn new_trace(&self, name: &str) -> PerformanceResult<TraceHandle> {
        if name.trim().is_empty() {
            return Err(invalid_argument("Trace name must not be empty"));
        }
        Ok(TraceHandle {
            performance: self.clone(),
            name: name.to_string(),
            start: Instant::now(),
            metrics: HashMap::new(),
        })
    }

    pub fn recorded_trace(&self, name: &str) -> Option<PerformanceTrace> {
        self.inner.traces.lock().unwrap().get(name).cloned()
    }
}

impl TraceHandle {
    pub fn put_metric(&mut self, name: &str, value: i64) -> PerformanceResult<()> {
        if name.trim().is_empty() {
            return Err(invalid_argument("Metric name must not be empty"));
        }
        self.metrics.insert(name.to_string(), value);
        Ok(())
    }

    pub fn stop(self) -> PerformanceResult<PerformanceTrace> {
        let duration = self.start.elapsed();
        let trace = PerformanceTrace {
            name: self.name.clone(),
            duration,
            metrics: self.metrics.clone(),
        };
        self.performance
            .inner
            .traces
            .lock()
            .unwrap()
            .insert(self.name.clone(), trace.clone());
        Ok(trace)
    }
}

static PERFORMANCE_COMPONENT: LazyLock<()> = LazyLock::new(|| {
    let component = Component::new(
        PERFORMANCE_COMPONENT_NAME,
        Arc::new(performance_factory),
        ComponentType::Public,
    )
    .with_instantiation_mode(InstantiationMode::Lazy);
    let _ = app::registry::register_component(component);
});

fn performance_factory(
    container: &crate::component::ComponentContainer,
    _options: InstanceFactoryOptions,
) -> Result<DynService, ComponentError> {
    let app = container.root_service::<FirebaseApp>().ok_or_else(|| {
        ComponentError::InitializationFailed {
            name: PERFORMANCE_COMPONENT_NAME.to_string(),
            reason: "Firebase app not attached to component container".to_string(),
        }
    })?;

    let performance = Performance::new((*app).clone());
    Ok(Arc::new(performance) as DynService)
}

fn ensure_registered() {
    LazyLock::force(&PERFORMANCE_COMPONENT);
}

pub fn register_performance_component() {
    ensure_registered();
}

pub fn get_performance(app: Option<FirebaseApp>) -> PerformanceResult<Arc<Performance>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => crate::app::api::get_app(None).map_err(|err| internal_error(err.to_string()))?,
    };

    let provider = app::registry::get_provider(&app, PERFORMANCE_COMPONENT_NAME);
    if let Some(perf) = provider.get_immediate::<Performance>() {
        return Ok(perf);
    }

    match provider.initialize::<Performance>(serde_json::Value::Null, None) {
        Ok(perf) => Ok(perf),
        Err(crate::component::types::ComponentError::InstanceUnavailable { .. }) => provider
            .get_immediate::<Performance>()
            .ok_or_else(|| internal_error("Performance component not available")),
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
                "performance-{}",
                COUNTER.fetch_add(1, Ordering::SeqCst)
            )),
            ..Default::default()
        }
    }

    #[test]
    fn trace_records_duration_and_metrics() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let performance = get_performance(Some(app)).unwrap();
        let mut trace = performance.new_trace("load").unwrap();
        trace.put_metric("items", 3).unwrap();
        std::thread::sleep(Duration::from_millis(10));
        let result = trace.stop().unwrap();
        assert_eq!(result.metrics.get("items"), Some(&3));
        assert!(result.duration >= Duration::from_millis(10));

        let stored = performance.recorded_trace("load").unwrap();
        assert_eq!(stored.metrics.get("items"), Some(&3));
    }
}
