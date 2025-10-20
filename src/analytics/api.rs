use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};

use crate::analytics::constants::ANALYTICS_COMPONENT_NAME;
use crate::analytics::error::{internal_error, invalid_argument, AnalyticsResult};
use crate::analytics::transport::{MeasurementProtocolConfig, MeasurementProtocolDispatcher};
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
    client_id: Mutex<String>,
    transport: Mutex<Option<MeasurementProtocolDispatcher>>,
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
            client_id: Mutex::new(generate_client_id()),
            transport: Mutex::new(None),
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
        let event = AnalyticsEvent {
            name: name.to_string(),
            params,
        };
        events.push(event.clone());
        drop(events);

        self.dispatch_event(&event)
    }

    pub fn recorded_events(&self) -> Vec<AnalyticsEvent> {
        self.inner.events.lock().unwrap().clone()
    }

    /// Configures the analytics instance to forward events using the GA4 Measurement Protocol.
    ///
    /// The configuration requires a valid measurement ID and API secret generated from the
    /// associated Google Analytics property. If a dispatcher has already been configured it is
    /// replaced.
    pub fn configure_measurement_protocol(
        &self,
        config: MeasurementProtocolConfig,
    ) -> AnalyticsResult<()> {
        let dispatcher = MeasurementProtocolDispatcher::new(config)?;
        let mut transport = self.inner.transport.lock().unwrap();
        *transport = Some(dispatcher);
        Ok(())
    }

    /// Overrides the client identifier reported to the measurement protocol. When unset the
    /// analytics instance falls back to a randomly generated identifier created during
    /// initialization.
    pub fn set_client_id(&self, client_id: impl Into<String>) {
        *self.inner.client_id.lock().unwrap() = client_id.into();
    }

    fn dispatch_event(&self, event: &AnalyticsEvent) -> AnalyticsResult<()> {
        let transport = {
            let guard = self.inner.transport.lock().unwrap();
            guard.clone()
        };

        if let Some(transport) = transport {
            let client_id = self.inner.client_id.lock().unwrap().clone();
            transport.send_event(&client_id, &event.name, &event.params)?
        }

        Ok(())
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

fn generate_client_id() -> String {
    use rand::distributions::Alphanumeric;
    use rand::Rng;

    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .map(char::from)
        .take(32)
        .collect()
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
    use crate::analytics::transport::MeasurementProtocolEndpoint;
    use crate::analytics::MeasurementProtocolConfig;
    use crate::app::api::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use httpmock::prelude::*;
    use std::collections::BTreeMap;
    use std::time::Duration;

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

    #[test]
    fn measurement_protocol_dispatches_events() {
        if std::env::var("FIREBASE_NETWORK_TESTS").is_err() {
            eprintln!(
                "skipping measurement_protocol_dispatches_events: set FIREBASE_NETWORK_TESTS=1 to enable"
            );
            return;
        }

        let server = match std::panic::catch_unwind(|| MockServer::start()) {
            Ok(server) => server,
            Err(_) => {
                eprintln!(
                    "skipping measurement_protocol_dispatches_events: sandbox forbids binding sockets"
                );
                return;
            }
        };
        let collect_path = "/mp/collect";
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path(collect_path)
                .query_param("measurement_id", "G-TEST123")
                .query_param("api_secret", "secret-key");
            then.status(204);
        });

        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let analytics = get_analytics(Some(app)).unwrap();

        let endpoint_url = format!(
            "{}/{}",
            server.base_url().trim_end_matches('/'),
            collect_path.trim_start_matches('/')
        );

        let config = MeasurementProtocolConfig::new("G-TEST123", "secret-key")
            .with_endpoint(MeasurementProtocolEndpoint::Custom(endpoint_url))
            .with_timeout(Duration::from_secs(2));
        analytics.configure_measurement_protocol(config).unwrap();
        analytics.set_client_id("client-123");

        let mut params = BTreeMap::new();
        params.insert("engagement_time_msec".to_string(), "100".to_string());
        analytics.log_event("test_event", params).unwrap();

        mock.assert();
    }
}
