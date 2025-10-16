use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};

use crate::analytics::constants::ANALYTICS_COMPONENT_NAME;
use crate::analytics::error::{internal_error, invalid_argument, AnalyticsResult};
use crate::app;
use crate::app::FirebaseApp;
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType};

#[derive(Clone, Debug)]
pub struct Analytics {
    inner: Arc<AnalyticsInner>,
}

#[derive(Debug)]
struct AnalyticsInner {
    app: FirebaseApp,
    events: Mutex<Vec<AnalyticsEvent>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalyticsEvent {
    pub name: String,
    pub params: BTreeMap<String, String>,
}

impl Analytics {
    fn new(app: FirebaseApp) -> Self {
        let inner = AnalyticsInner {
            app,
            events: Mutex::new(Vec::new()),
        };
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn app(&self) -> &FirebaseApp {
        &self.inner.app
    }

    pub fn log_event(&self, name: &str, params: BTreeMap<String, String>) -> AnalyticsResult<()> {
        validate_event_name(name)?;
        let mut events = self.inner.events.lock().unwrap();
        events.push(AnalyticsEvent {
            name: name.to_string(),
            params,
        });
        Ok(())
    }

    pub fn recorded_events(&self) -> Vec<AnalyticsEvent> {
        self.inner.events.lock().unwrap().clone()
    }
}

fn validate_event_name(name: &str) -> AnalyticsResult<()> {
    if name.trim().is_empty() {
        return Err(invalid_argument("Event name must not be empty"));
    }
    Ok(())
}

static ANALYTICS_COMPONENT: LazyLock<Component> = LazyLock::new(|| {
    Component::new(
        ANALYTICS_COMPONENT_NAME,
        Arc::new(analytics_factory),
        ComponentType::Public,
    )
    .with_instantiation_mode(InstantiationMode::Lazy)
});

fn analytics_factory(
    container: &crate::component::ComponentContainer,
    _options: InstanceFactoryOptions,
) -> Result<DynService, ComponentError> {
    let app = container.root_service::<FirebaseApp>().ok_or_else(|| {
        ComponentError::InitializationFailed {
            name: ANALYTICS_COMPONENT_NAME.to_string(),
            reason: "Firebase app not attached to component container".to_string(),
        }
    })?;
    let analytics = Analytics::new((*app).clone());
    Ok(Arc::new(analytics) as DynService)
}

fn ensure_registered() {
    let component = LazyLock::force(&ANALYTICS_COMPONENT).clone();
    let _ = app::registry::register_component(component);
}

pub fn register_analytics_component() {
    ensure_registered();
}

pub fn get_analytics(app: Option<FirebaseApp>) -> AnalyticsResult<Arc<Analytics>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => crate::app::api::get_app(None).map_err(|err| internal_error(err.to_string()))?,
    };

    let provider = app::registry::get_provider(&app, ANALYTICS_COMPONENT_NAME);
    provider
        .get_immediate::<Analytics>()
        .ok_or_else(|| internal_error("Analytics component not available"))
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
                "analytics-{}",
                COUNTER.fetch_add(1, Ordering::SeqCst)
            )),
            ..Default::default()
        }
    }

    #[test]
    fn log_event_records_entry() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let analytics = get_analytics(Some(app)).unwrap();
        let mut params = BTreeMap::new();
        params.insert("origin".into(), "test".into());
        analytics.log_event("test_event", params.clone()).unwrap();
        let events = analytics.recorded_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "test_event");
        assert_eq!(events[0].params, params);
    }
}
