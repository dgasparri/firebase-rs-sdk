use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use async_trait::async_trait;

use crate::analytics::config::{fetch_dynamic_config, from_app_options, DynamicConfig};
use crate::analytics::constants::ANALYTICS_COMPONENT_NAME;
use crate::analytics::error::{internal_error, invalid_argument, AnalyticsResult};
use crate::analytics::gtag::{GlobalGtagRegistry, GtagState};
use crate::analytics::transport::{
    MeasurementProtocolConfig, MeasurementProtocolDispatcher, MeasurementProtocolEndpoint,
};
use crate::app;
use crate::app::FirebaseApp;
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType};

#[derive(Clone)]
pub struct Analytics {
    inner: Arc<AnalyticsInner>,
}

impl fmt::Debug for Analytics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Analytics")
            .field("app", &self.inner.app.name())
            .finish()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnalyticsSettings {
    pub config: BTreeMap<String, String>,
    pub send_page_view: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConsentSettings {
    pub entries: BTreeMap<String, String>,
}

struct AnalyticsInner {
    app: FirebaseApp,
    events: Mutex<Vec<AnalyticsEvent>>,
    client_id: Mutex<String>,
    transport: Mutex<Option<Arc<dyn AnalyticsTransport>>>,
    config: Mutex<Option<DynamicConfig>>,
    default_event_params: Mutex<BTreeMap<String, String>>,
    consent_settings: Mutex<Option<ConsentSettings>>,
    analytics_settings: Mutex<AnalyticsSettings>,
    collection_enabled: AtomicBool,
    gtag: GlobalGtagRegistry,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalyticsEvent {
    pub name: String,
    pub params: BTreeMap<String, String>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
trait AnalyticsTransport: Send + Sync {
    async fn send(
        &self,
        client_id: &str,
        event_name: &str,
        params: &BTreeMap<String, String>,
    ) -> AnalyticsResult<()>;
}

#[derive(Clone)]
struct MeasurementProtocolTransport {
    dispatcher: MeasurementProtocolDispatcher,
}

impl MeasurementProtocolTransport {
    fn new(dispatcher: MeasurementProtocolDispatcher) -> Self {
        Self { dispatcher }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl AnalyticsTransport for MeasurementProtocolTransport {
    async fn send(
        &self,
        client_id: &str,
        event_name: &str,
        params: &BTreeMap<String, String>,
    ) -> AnalyticsResult<()> {
        self.dispatcher
            .send_event(client_id, event_name, params)
            .await
    }
}

impl Analytics {
    fn new(app: FirebaseApp) -> Self {
        let gtag = GlobalGtagRegistry::shared();
        gtag.inner().set_data_layer_name("dataLayer");

        let inner = AnalyticsInner {
            app,
            events: Mutex::new(Vec::new()),
            client_id: Mutex::new(generate_client_id()),
            transport: Mutex::new(None),
            config: Mutex::new(None),
            default_event_params: Mutex::new(BTreeMap::new()),
            consent_settings: Mutex::new(None),
            analytics_settings: Mutex::new(AnalyticsSettings::default()),
            collection_enabled: AtomicBool::new(true),
            gtag,
        };
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn app(&self) -> &FirebaseApp {
        &self.inner.app
    }

    pub async fn log_event(
        &self,
        name: &str,
        params: BTreeMap<String, String>,
    ) -> AnalyticsResult<()> {
        validate_event_name(name)?;
        let merged_params = self.merge_default_event_params(params);
        let mut events = self.inner.events.lock().unwrap();
        let event = AnalyticsEvent {
            name: name.to_string(),
            params: merged_params,
        };
        events.push(event.clone());
        drop(events);

        self.dispatch_event(&event).await
    }

    pub fn recorded_events(&self) -> Vec<AnalyticsEvent> {
        self.inner.events.lock().unwrap().clone()
    }

    /// Returns a snapshot of the gtag bootstrap state collected so far.
    pub fn gtag_state(&self) -> GtagState {
        self.inner.gtag.inner().snapshot()
    }

    /// Resolves the measurement configuration for this analytics instance. The value is derived
    /// from the Firebase app options when possible and otherwise fetched from the Firebase
    /// analytics REST endpoint. Results are cached for subsequent calls.
    pub async fn measurement_config(&self) -> AnalyticsResult<DynamicConfig> {
        self.ensure_dynamic_config().await
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
        *transport = Some(Arc::new(MeasurementProtocolTransport::new(dispatcher)));
        Ok(())
    }

    /// Convenience helper that resolves the measurement configuration and configures the
    /// measurement protocol using the provided API secret. The dispatcher targets the default GA4
    /// collection endpoint.
    pub async fn configure_measurement_protocol_with_secret(
        &self,
        api_secret: impl Into<String>,
    ) -> AnalyticsResult<()> {
        self.configure_measurement_protocol_with_secret_internal(api_secret, None)
            .await
    }

    /// Convenience helper that resolves the measurement configuration and configures the
    /// measurement protocol using the provided API secret and custom endpoint. This is primarily
    /// intended for testing or emulator scenarios.
    pub async fn configure_measurement_protocol_with_secret_and_endpoint(
        &self,
        api_secret: impl Into<String>,
        endpoint: MeasurementProtocolEndpoint,
    ) -> AnalyticsResult<()> {
        self.configure_measurement_protocol_with_secret_internal(api_secret, Some(endpoint))
            .await
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
        *self.inner.default_event_params.lock().unwrap() = params.clone();
        self.inner.gtag.inner().set_default_event_parameters(params);
    }

    /// Configures default consent settings that mirror the GA4 consent API. The values are cached
    /// so they can be applied once full gtag integration is implemented. Calling this replaces any
    /// previously stored consent state.
    pub fn set_consent_defaults(&self, consent: ConsentSettings) {
        let entries = consent.entries.clone();
        *self.inner.consent_settings.lock().unwrap() = Some(consent);
        self.inner.gtag.inner().set_consent_defaults(Some(entries));
    }

    /// Applies analytics configuration options analogous to the JS `AnalyticsSettings` structure.
    /// The configuration is cached and merged with any previously supplied settings.
    pub fn apply_settings(&self, settings: AnalyticsSettings) {
        let mut guard = self.inner.analytics_settings.lock().unwrap();
        for (key, value) in settings.config {
            guard.config.insert(key, value);
        }
        if settings.send_page_view.is_some() {
            guard.send_page_view = settings.send_page_view;
        }
        self.inner.gtag.inner().set_config(guard.config.clone());
        self.inner
            .gtag
            .inner()
            .set_send_page_view(guard.send_page_view);
    }

    async fn dispatch_event(&self, event: &AnalyticsEvent) -> AnalyticsResult<()> {
        let transport = {
            let guard = self.inner.transport.lock().unwrap();
            guard.clone()
        };

        if self.inner.collection_enabled.load(Ordering::SeqCst) {
            if let Some(transport) = transport {
                let client_id = self.inner.client_id.lock().unwrap().clone();
                transport
                    .send(&client_id, &event.name, &event.params)
                    .await?
            }
        }

        Ok(())
    }

    async fn configure_measurement_protocol_with_secret_internal(
        &self,
        api_secret: impl Into<String>,
        endpoint: Option<MeasurementProtocolEndpoint>,
    ) -> AnalyticsResult<()> {
        let config = self.ensure_dynamic_config().await?;
        let mut mp_config =
            MeasurementProtocolConfig::new(config.measurement_id().to_string(), api_secret);
        if let Some(endpoint) = endpoint {
            mp_config = mp_config.with_endpoint(endpoint);
        }
        self.configure_measurement_protocol(mp_config)
    }

    async fn ensure_dynamic_config(&self) -> AnalyticsResult<DynamicConfig> {
        if let Some(cached) = self.inner.config.lock().unwrap().clone() {
            return Ok(cached);
        }

        if let Some(local) = from_app_options(&self.inner.app) {
            let mut guard = self.inner.config.lock().unwrap();
            *guard = Some(local.clone());
            self.inner
                .gtag
                .inner()
                .set_measurement_id(Some(local.measurement_id().to_string()));
            return Ok(local);
        }

        // Fetch without holding the config mutex to avoid blocking other readers while awaiting.
        let fetched = fetch_dynamic_config(&self.inner.app).await?;
        let mut guard = self.inner.config.lock().unwrap();
        *guard = Some(fetched.clone());
        self.inner
            .gtag
            .inner()
            .set_measurement_id(Some(fetched.measurement_id().to_string()));
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

    /// Enables or disables analytics collection. When disabled, events are still recorded locally
    /// but are not dispatched through the configured transport.
    pub fn set_collection_enabled(&self, enabled: bool) {
        self.inner
            .collection_enabled
            .store(enabled, Ordering::SeqCst);
    }

    /// Returns whether analytics collection is currently enabled.
    pub fn collection_enabled(&self) -> bool {
        self.inner.collection_enabled.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    fn set_transport_for_tests(&self, transport: Arc<dyn AnalyticsTransport>) {
        *self.inner.transport.lock().unwrap() = Some(transport);
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

pub async fn get_analytics(app: Option<FirebaseApp>) -> AnalyticsResult<Arc<Analytics>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => crate::app::api::get_app(None)
            .await
            .map_err(|err| internal_error(err.to_string()))?,
    };

    let provider = app::registry::get_provider(&app, ANALYTICS_COMPONENT_NAME);
    provider
        .get_immediate::<Analytics>()
        .ok_or_else(|| internal_error("Analytics component not available"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::gtag::GlobalGtagRegistry;
    use crate::app::api::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use std::collections::BTreeMap;
    use std::sync::{Arc, LazyLock, Mutex};

    static GTAG_TEST_MUTEX: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

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

    fn reset_gtag_state() {
        GlobalGtagRegistry::shared().inner().reset();
    }

    fn gtag_test_guard() -> std::sync::MutexGuard<'static, ()> {
        GTAG_TEST_MUTEX.lock().unwrap()
    }

    #[derive(Default, Clone)]
    struct RecordingTransport {
        events: Arc<Mutex<Vec<(String, BTreeMap<String, String>)>>>,
        clients: Arc<Mutex<Vec<String>>>,
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl AnalyticsTransport for RecordingTransport {
        async fn send(
            &self,
            client_id: &str,
            event_name: &str,
            params: &BTreeMap<String, String>,
        ) -> AnalyticsResult<()> {
            self.clients.lock().unwrap().push(client_id.to_string());
            self.events
                .lock()
                .unwrap()
                .push((event_name.to_string(), params.clone()));
            Ok(())
        }
    }

    impl RecordingTransport {
        fn take_events(&self) -> Vec<(String, BTreeMap<String, String>)> {
            self.events.lock().unwrap().clone()
        }

        #[allow(dead_code)]
        fn clients(&self) -> Vec<String> {
            self.clients.lock().unwrap().clone()
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn log_event_records_entry() {
        let _guard = gtag_test_guard();
        reset_gtag_state();
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            measurement_id: Some("G-LOCAL123".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .unwrap();
        let analytics = get_analytics(Some(app)).await.unwrap();
        let mut params = BTreeMap::new();
        params.insert("origin".into(), "test".into());
        analytics
            .log_event("test_event", params.clone())
            .await
            .unwrap();
        let events = analytics.recorded_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "test_event");
        assert_eq!(events[0].params, params);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn default_event_parameters_are_applied() {
        let _guard = gtag_test_guard();
        reset_gtag_state();
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            measurement_id: Some("G-LOCAL789".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .unwrap();
        let analytics = get_analytics(Some(app)).await.unwrap();
        analytics.set_default_event_parameters(BTreeMap::from([(
            "origin".to_string(),
            "default".to_string(),
        )]));

        let mut params = BTreeMap::new();
        params.insert("value".into(), "42".into());
        analytics.log_event("test", params).await.unwrap();

        let events = analytics.recorded_events();
        let recorded = &events[0];
        assert_eq!(recorded.params.get("origin"), Some(&"default".to_string()));
        assert_eq!(recorded.params.get("value"), Some(&"42".to_string()));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn default_event_parameters_do_not_override_explicit_values() {
        let _guard = gtag_test_guard();
        reset_gtag_state();
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            measurement_id: Some("G-LOCAL990".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .unwrap();
        let analytics = get_analytics(Some(app)).await.unwrap();
        analytics.set_default_event_parameters(BTreeMap::from([(
            "value".to_string(),
            "default".to_string(),
        )]));

        let mut params = BTreeMap::new();
        params.insert("value".into(), "custom".into());
        analytics.log_event("test", params).await.unwrap();

        let events = analytics.recorded_events();
        let recorded = &events[0];
        assert_eq!(recorded.params.get("value"), Some(&"custom".to_string()));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn measurement_config_uses_local_options() {
        let _guard = gtag_test_guard();
        reset_gtag_state();
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            measurement_id: Some("G-LOCAL456".into()),
            app_id: Some("1:123:web:abc".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .unwrap();
        let analytics = get_analytics(Some(app)).await.unwrap();

        let config = analytics.measurement_config().await.unwrap();
        assert_eq!(config.measurement_id(), "G-LOCAL456");
        assert_eq!(config.app_id(), Some("1:123:web:abc"));

        let gtag_state = analytics.gtag_state();
        assert_eq!(gtag_state.measurement_id, Some("G-LOCAL456".to_string()));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn configure_with_secret_requires_measurement_context() {
        let _guard = gtag_test_guard();
        reset_gtag_state();
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .unwrap();
        let analytics = get_analytics(Some(app)).await.unwrap();

        let err = analytics
            .configure_measurement_protocol_with_secret("secret")
            .await
            .unwrap_err();
        assert_eq!(err.code_str(), "analytics/missing-measurement-id");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn collection_toggle_controls_state() {
        let _guard = gtag_test_guard();
        reset_gtag_state();
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            measurement_id: Some("G-LOCALCOLLECT".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .unwrap();
        let analytics = get_analytics(Some(app)).await.unwrap();

        assert!(analytics.collection_enabled());
        analytics.set_collection_enabled(false);
        assert!(!analytics.collection_enabled());
        analytics.set_collection_enabled(true);
        assert!(analytics.collection_enabled());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn gtag_state_tracks_defaults_and_config() {
        let _guard = gtag_test_guard();
        reset_gtag_state();
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            measurement_id: Some("G-GTAGTEST".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .unwrap();
        let analytics = get_analytics(Some(app)).await.unwrap();

        analytics.set_default_event_parameters(BTreeMap::from([(
            "currency".to_string(),
            "USD".to_string(),
        )]));
        analytics.set_consent_defaults(ConsentSettings {
            entries: BTreeMap::from([(String::from("ad_storage"), String::from("granted"))]),
        });
        analytics.apply_settings(AnalyticsSettings {
            config: BTreeMap::from([(String::from("send_page_view"), String::from("false"))]),
            send_page_view: Some(false),
        });
        // Force measurement configuration resolution so the gtag registry is populated.
        analytics.measurement_config().await.unwrap();

        let state = analytics.gtag_state();
        assert_eq!(state.data_layer_name, "dataLayer");
        assert_eq!(state.measurement_id, Some("G-GTAGTEST".to_string()));
        assert_eq!(
            state.default_event_parameters.get("currency"),
            Some(&"USD".to_string())
        );
        assert_eq!(
            state
                .consent_settings
                .as_ref()
                .and_then(|m| m.get("ad_storage")),
            Some(&"granted".to_string())
        );
        assert_eq!(state.send_page_view, Some(false));
        assert_eq!(
            state.config.get("send_page_view"),
            Some(&"false".to_string())
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn measurement_protocol_dispatches_events() {
        let _guard = gtag_test_guard();
        reset_gtag_state();

        let options = FirebaseOptions {
            project_id: Some("project".into()),
            measurement_id: Some("G-TEST123".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .unwrap();
        let analytics = get_analytics(Some(app)).await.unwrap();

        let transport = RecordingTransport::default();
        analytics.set_transport_for_tests(Arc::new(transport.clone()));
        analytics.set_client_id("client-123");

        let mut params = BTreeMap::new();
        params.insert("engagement_time_msec".to_string(), "100".to_string());

        analytics
            .log_event("test_event", params.clone())
            .await
            .unwrap();

        let events = transport.take_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "test_event");
        assert_eq!(events[0].1, params);
    }
}
