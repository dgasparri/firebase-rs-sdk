use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use crate::app;
use crate::app::FirebaseApp;
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType};
use crate::performance::constants::PERFORMANCE_COMPONENT_NAME;
use crate::performance::error::{internal_error, invalid_argument, PerformanceResult};
use async_lock::Mutex;

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

    /// Returns the [`FirebaseApp`] that owns this Performance monitor.
    pub fn app(&self) -> &FirebaseApp {
        &self.inner.app
    }

    /// Creates a new manual trace, mirroring the JS SDK's `trace()` helper.
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

    /// Returns the most recently recorded trace with `name`, if any.
    pub async fn recorded_trace(&self, name: &str) -> Option<PerformanceTrace> {
        self.inner.traces.lock().await.get(name).cloned()
    }
}

impl TraceHandle {
    /// Adds (or replaces) a numeric metric for the trace.
    pub fn put_metric(&mut self, name: &str, value: i64) -> PerformanceResult<()> {
        if name.trim().is_empty() {
            return Err(invalid_argument("Metric name must not be empty"));
        }
        self.metrics.insert(name.to_string(), value);
        Ok(())
    }

    /// Stops the trace and stores the timing/metrics in the parent [`Performance`] instance.
    pub async fn stop(self) -> PerformanceResult<PerformanceTrace> {
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
            .await
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
    let _ = app::register_component(component);
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

/// Resolves (or lazily creates) the [`Performance`] instance associated with the provided app.
///
/// This mirrors the behaviour of the JavaScript SDK's `getPerformance` helper. When `app` is
/// `None`, the default app is resolved asynchronously via [`get_app`](crate::app::get_app).
pub async fn get_performance(app: Option<FirebaseApp>) -> PerformanceResult<Arc<Performance>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => crate::app::get_app(None)
            .await
            .map_err(|err| internal_error(err.to_string()))?,
    };

    let provider = app::get_provider(&app, PERFORMANCE_COMPONENT_NAME);
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::app::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use tokio::time::sleep;

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

    #[tokio::test(flavor = "current_thread")]
    async fn trace_records_duration_and_metrics() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .unwrap();
        let performance = get_performance(Some(app.clone())).await.unwrap();
        let mut trace = performance.new_trace("load").unwrap();
        trace.put_metric("items", 3).unwrap();
        sleep(Duration::from_millis(10)).await;
        let result = trace.stop().await.unwrap();
        assert_eq!(result.metrics.get("items"), Some(&3));
        assert!(result.duration >= Duration::from_millis(10));

        let stored = performance.recorded_trace("load").await.unwrap();
        assert_eq!(stored.metrics.get("items"), Some(&3));
    }
}
