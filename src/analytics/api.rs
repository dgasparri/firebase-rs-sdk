use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};

use crate::analytics::config::{fetch_dynamic_config, from_app_options, DynamicConfig};
use crate::analytics::constants::ANALYTICS_COMPONENT_NAME;
use crate::analytics::error::{internal_error, invalid_argument, AnalyticsResult};
use crate::analytics::transport::{
    MeasurementProtocolConfig, MeasurementProtocolDispatcher, MeasurementProtocolEndpoint,
};
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnalyticsSettings {
    pub config: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConsentSettings {
    pub entries: BTreeMap<String, String>,
}

#[derive(Debug)]
struct AnalyticsInner {
    app: FirebaseApp,
    events: Mutex<Vec<AnalyticsEvent>>,
    client_id: Mutex<String>,
    transport: Mutex<Option<MeasurementProtocolDispatcher>>,
    config: Mutex<Option<DynamicConfig>>,
    default_event_params: Mutex<BTreeMap<String, String>>,
    consent_settings: Mutex<Option<ConsentSettings>>,
    analytics_settings: Mutex<AnalyticsSettings>,
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
            config: Mutex::new(None),
            default_event_params: Mutex::new(BTreeMap::new()),
            consent_settings: Mutex::new(None),
            analytics_settings: Mutex::new(AnalyticsSettings::default()),
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
        let merged_params = self.merge_default_event_params(params);
        let mut events = self.inner.events.lock().unwrap();
        let event = AnalyticsEvent {
            name: name.to_string(),
            params: merged_params,
        };
        events.push(event.clone());
        drop(events);

        self.dispatch_event(&event)
    }

    pub fn recorded_events(&self) -> Vec<AnalyticsEvent> {
        self.inner.events.lock().unwrap().clone()
    }

    /// Resolves the measurement configuration for this analytics instance. The value is derived
    /// from the Firebase app options when possible and otherwise fetched from the Firebase
    /// analytics REST endpoint. Results are cached for subsequent calls.
    pub fn measurement_config(&self) -> AnalyticsResult<DynamicConfig> {
        self.ensure_dynamic_config()
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

    /// Convenience helper that resolves the measurement configuration and configures the
    /// measurement protocol using the provided API secret. The dispatcher targets the default GA4
    /// collection endpoint.
    pub fn configure_measurement_protocol_with_secret(
        &self,
        api_secret: impl Into<String>,
    ) -> AnalyticsResult<()> {
        self.configure_measurement_protocol_with_secret_internal(api_secret, None)
    }

    /// Convenience helper that resolves the measurement configuration and configures the
    /// measurement protocol using the provided API secret and custom endpoint. This is primarily
    /// intended for testing or emulator scenarios.
    pub fn configure_measurement_protocol_with_secret_and_endpoint(
        &self,
        api_secret: impl Into<String>,
        endpoint: MeasurementProtocolEndpoint,
    ) -> AnalyticsResult<()> {
        self.configure_measurement_protocol_with_secret_internal(api_secret, Some(endpoint))
    }

    /// Overrides the client identifier reported to the measurement protocol. When unset the
    /// analytics instance falls back to a randomly generated identifier created during
    /// initialization.
    pub fn set_client_id(&self, client_id: impl Into<String>) {
        *self.inner.client_id.lock().unwrap() = client_id.into();
    }

    /// Sets the default event parameters that should be merged into every logged event unless
    /// explicitly overridden.
    pub fn set_default_event_parameters(&self, params: BTreeMap<String, String>) {
        *self.inner.default_event_params.lock().unwrap() = params;
    }

    /// Configures default consent settings that mirror the GA4 consent API. The values are cached
    /// so they can be applied once full gtag integration is implemented. Calling this replaces any
    /// previously stored consent state.
    pub fn set_consent_defaults(&self, consent: ConsentSettings) {
        *self.inner.consent_settings.lock().unwrap() = Some(consent);
    }

    /// Applies analytics configuration options analogous to the JS `AnalyticsSettings` structure.
    /// The configuration is cached and can be inspected later.
    pub fn apply_settings(&self, settings: AnalyticsSettings) {
        *self.inner.analytics_settings.lock().unwrap() = settings;
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

    fn configure_measurement_protocol_with_secret_internal(
        &self,
        api_secret: impl Into<String>,
        endpoint: Option<MeasurementProtocolEndpoint>,
    ) -> AnalyticsResult<()> {
        let config = self.ensure_dynamic_config()?;
        let mut mp_config =
            MeasurementProtocolConfig::new(config.measurement_id().to_string(), api_secret);
        if let Some(endpoint) = endpoint {
            mp_config = mp_config.with_endpoint(endpoint);
        }
        self.configure_measurement_protocol(mp_config)
    }

    fn ensure_dynamic_config(&self) -> AnalyticsResult<DynamicConfig> {
        if let Some(cached) = self.inner.config.lock().unwrap().clone() {
            return Ok(cached);
        }

        if let Some(local) = from_app_options(&self.inner.app) {
            let mut guard = self.inner.config.lock().unwrap();
            *guard = Some(local.clone());
            return Ok(local);
        }

        let fetched = fetch_dynamic_config(&self.inner.app)?;
        let mut guard = self.inner.config.lock().unwrap();
        *guard = Some(fetched.clone());
        Ok(fetched)
    }

    fn merge_default_event_params(
        &self,
        mut params: BTreeMap<String, String>,
    ) -> BTreeMap<String, String> {
        let defaults = self.inner.default_event_params.lock().unwrap().clone();
        for (key, value) in defaults {
            params.entry(key).or_insert(value);
        }
        params
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
    use crate::app::api::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use httpmock::prelude::*;
    use std::collections::BTreeMap;

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
            measurement_id: Some("G-LOCAL123".into()),
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
    fn default_event_parameters_are_applied() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            measurement_id: Some("G-LOCAL789".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let analytics = get_analytics(Some(app)).unwrap();
        analytics.set_default_event_parameters(BTreeMap::from([(
            "origin".to_string(),
            "default".to_string(),
        )]));

        let mut params = BTreeMap::new();
        params.insert("value".into(), "42".into());
        analytics.log_event("test", params).unwrap();

        let events = analytics.recorded_events();
        let recorded = &events[0];
        assert_eq!(recorded.params.get("origin"), Some(&"default".to_string()));
        assert_eq!(recorded.params.get("value"), Some(&"42".to_string()));
    }

    #[test]
    fn default_event_parameters_do_not_override_explicit_values() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            measurement_id: Some("G-LOCAL990".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let analytics = get_analytics(Some(app)).unwrap();
        analytics.set_default_event_parameters(BTreeMap::from([(
            "value".to_string(),
            "default".to_string(),
        )]));

        let mut params = BTreeMap::new();
        params.insert("value".into(), "custom".into());
        analytics.log_event("test", params).unwrap();

        let events = analytics.recorded_events();
        let recorded = &events[0];
        assert_eq!(recorded.params.get("value"), Some(&"custom".to_string()));
    }

    #[test]
    fn measurement_config_uses_local_options() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            measurement_id: Some("G-LOCAL456".into()),
            app_id: Some("1:123:web:abc".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let analytics = get_analytics(Some(app)).unwrap();

        let config = analytics.measurement_config().unwrap();
        assert_eq!(config.measurement_id(), "G-LOCAL456");
        assert_eq!(config.app_id(), Some("1:123:web:abc"));
    }

    #[test]
    fn configure_with_secret_requires_measurement_context() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let analytics = get_analytics(Some(app)).unwrap();

        let err = analytics
            .configure_measurement_protocol_with_secret("secret")
            .unwrap_err();
        assert_eq!(err.code_str(), "analytics/missing-measurement-id");
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
        let mock_collect = server.mock(|when, then| {
            when.method(POST)
                .path(collect_path)
                .query_param("measurement_id", "G-TEST123")
                .query_param("api_secret", "secret-key");
            then.status(204);
        });

        let config_path = "/v1alpha/projects/-/apps/app-123/webConfig";
        let mock_config = server.mock(|when, then| {
            when.method(GET)
                .path(config_path)
                .header("x-goog-api-key", "api-key");
            then.status(200).json_body(serde_json::json!({
                "measurementId": "G-TEST123",
                "appId": "app-123"
            }));
        });

        let options = FirebaseOptions {
            project_id: Some("project".into()),
            app_id: Some("app-123".into()),
            api_key: Some("api-key".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let analytics = get_analytics(Some(app)).unwrap();

        let endpoint_url = format!(
            "{}/{}",
            server.base_url().trim_end_matches('/'),
            collect_path.trim_start_matches('/')
        );

        let config_template = format!(
            "{}/{{app-id}}/webConfig",
            format!(
                "{}/v1alpha/projects/-/apps",
                server.base_url().trim_end_matches('/')
            )
        );
        std::env::set_var("FIREBASE_ANALYTICS_CONFIG_URL", config_template);

        analytics
            .configure_measurement_protocol_with_secret_and_endpoint(
                "secret-key",
                MeasurementProtocolEndpoint::Custom(endpoint_url),
            )
            .unwrap();

        analytics.set_client_id("client-123");

        let mut params = BTreeMap::new();
        params.insert("engagement_time_msec".to_string(), "100".to_string());
        analytics.log_event("test_event", params).unwrap();

        mock_config.assert();
        mock_collect.assert();

        std::env::remove_var("FIREBASE_ANALYTICS_CONFIG_URL");
    }
}
