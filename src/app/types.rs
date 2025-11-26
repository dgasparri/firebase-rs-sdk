use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::app::errors::{AppError, AppResult};
use crate::component::constants::DEFAULT_ENTRY_NAME;
use crate::component::types::DynService;
use crate::component::{Component, ComponentContainer};
use crate::platform::environment;
use crate::platform::runtime::spawn_detached;

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionService {
    pub library: String,
    pub version: String,
}

#[allow(dead_code)]
pub trait PlatformLoggerService: Send + Sync {
    fn platform_info_string(&self) -> String;
}

#[cfg_attr(
    all(feature = "wasm-web", target_arch = "wasm32"),
    async_trait::async_trait(?Send)
)]
#[cfg_attr(not(all(feature = "wasm-web", target_arch = "wasm32")), async_trait::async_trait)]
pub trait HeartbeatService: Send + Sync {
    async fn trigger_heartbeat(&self) -> AppResult<()>;
    #[allow(dead_code)]
    async fn heartbeats_header(&self) -> AppResult<Option<String>>;
}

#[cfg_attr(
    all(feature = "wasm-web", target_arch = "wasm32"),
    async_trait::async_trait(?Send)
)]
#[cfg_attr(not(all(feature = "wasm-web", target_arch = "wasm32")), async_trait::async_trait)]
pub trait HeartbeatStorage: Send + Sync {
    async fn read(&self) -> AppResult<HeartbeatsInStorage>;
    async fn overwrite(&self, value: &HeartbeatsInStorage) -> AppResult<()>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HeartbeatsInStorage {
    pub last_sent_heartbeat_date: Option<String>,
    pub heartbeats: Vec<SingleDateHeartbeat>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SingleDateHeartbeat {
    pub agent: String,
    pub date: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FirebaseOptions {
    pub api_key: Option<String>,
    pub auth_domain: Option<String>,
    pub database_url: Option<String>,
    pub project_id: Option<String>,
    pub storage_bucket: Option<String>,
    pub messaging_sender_id: Option<String>,
    pub app_id: Option<String>,
    pub measurement_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FirebaseAppSettings {
    pub name: Option<String>,
    pub automatic_data_collection_enabled: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirebaseAppConfig {
    pub name: Arc<str>,
    pub automatic_data_collection_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FirebaseServerAppSettings {
    pub automatic_data_collection_enabled: Option<bool>,
    pub auth_id_token: Option<String>,
    pub app_check_token: Option<String>,
    pub release_on_deref: Option<bool>,
}

#[derive(Clone)]
pub struct FirebaseApp {
    inner: Arc<FirebaseAppInner>,
}

struct FirebaseAppInner {
    options: FirebaseOptions,
    config: FirebaseAppConfig,
    automatic_data_collection_enabled: Mutex<bool>,
    is_deleted: AtomicBool,
    container: ComponentContainer,
}

impl FirebaseApp {
    /// Creates a new `FirebaseApp` from options, config, and the component container.
    pub fn new(options: FirebaseOptions, config: FirebaseAppConfig, container: ComponentContainer) -> Self {
        let automatic = config.automatic_data_collection_enabled;
        let inner = Arc::new(FirebaseAppInner {
            options,
            config,
            automatic_data_collection_enabled: Mutex::new(automatic),
            is_deleted: AtomicBool::new(false),
            container,
        });
        let app = Self { inner: inner.clone() };
        let dyn_service: DynService = Arc::new(app.clone());
        app.inner.container.attach_root_service(dyn_service);
        app
    }

    /// Returns the app's logical name.
    pub fn name(&self) -> &str {
        &self.inner.config.name
    }

    /// Provides a cloned copy of the original Firebase options.
    pub fn options(&self) -> FirebaseOptions {
        self.inner.options.clone()
    }

    /// Returns the configuration metadata associated with the app.
    pub fn config(&self) -> FirebaseAppConfig {
        self.inner.config.clone()
    }

    /// Indicates whether automatic data collection is currently enabled.
    pub fn automatic_data_collection_enabled(&self) -> bool {
        *self
            .inner
            .automatic_data_collection_enabled
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Updates the automatic data collection flag for the app.
    pub fn set_automatic_data_collection_enabled(&self, value: bool) {
        *self
            .inner
            .automatic_data_collection_enabled
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = value;
    }

    /// Exposes the component container for advanced service registration.
    pub fn container(&self) -> ComponentContainer {
        self.inner.container.clone()
    }

    /// Adds a lazily-initialized component to the app.
    pub fn add_component(&self, component: Component) -> AppResult<()> {
        self.check_destroyed()?;
        self.inner.container.add_component(component).map_err(AppError::from)
    }

    /// Adds a component, replacing any existing implementation with the same name.
    pub fn add_or_overwrite_component(&self, component: Component) -> AppResult<()> {
        self.check_destroyed()?;
        self.inner.container.add_or_overwrite_component(component);
        Ok(())
    }

    /// Removes a cached service instance from the specified provider.
    pub fn remove_service_instance(&self, name: &str, identifier: Option<&str>) {
        let provider = self.inner.container.get_provider(name);
        if let Some(id) = identifier {
            provider.clear_instance(id);
        } else {
            provider.clear_instance(DEFAULT_ENTRY_NAME);
        }
    }

    /// Returns whether the app has been explicitly deleted.
    pub fn is_deleted(&self) -> bool {
        self.inner.is_deleted.load(Ordering::SeqCst)
    }

    /// Marks the app as deleted (internal use).
    pub fn set_is_deleted(&self, value: bool) {
        self.inner.is_deleted.store(value, Ordering::SeqCst);
    }

    /// Verifies that the app has not been deleted before performing operations.
    pub fn check_destroyed(&self) -> AppResult<()> {
        if self.is_deleted() {
            return Err(AppError::AppDeleted {
                app_name: self.name().to_owned(),
            });
        }
        Ok(())
    }
}

impl std::fmt::Debug for FirebaseApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FirebaseApp")
            .field("name", &self.name())
            .field("automatic_data_collection_enabled", &self.automatic_data_collection_enabled())
            .finish()
    }
}

impl FirebaseAppConfig {
    /// Creates a configuration value capturing the app name and data collection setting.
    pub fn new(name: impl Into<String>, automatic: bool) -> Self {
        Self {
            name: to_arc_str(name),
            automatic_data_collection_enabled: automatic,
        }
    }
}

#[derive(Clone)]
pub struct FirebaseServerApp {
    inner: Arc<FirebaseServerAppInner>,
}

struct FirebaseServerAppInner {
    base: FirebaseApp,
    settings: FirebaseServerAppSettings,
    ref_count: AtomicUsize,
    release_on_drop: AtomicBool,
}

impl FirebaseServerApp {
    /// Wraps a `FirebaseApp` with server-specific settings and reference counting.
    pub fn new(base: FirebaseApp, mut settings: FirebaseServerAppSettings) -> Self {
        let release_on_drop = settings.release_on_deref.unwrap_or(false);
        settings.release_on_deref = None;
        base.set_is_deleted(false);

        Self {
            inner: Arc::new(FirebaseServerAppInner {
                base,
                settings,
                ref_count: AtomicUsize::new(1),
                release_on_drop: AtomicBool::new(release_on_drop),
            }),
        }
    }

    /// Returns the underlying base app instance.
    pub fn base(&self) -> &FirebaseApp {
        &self.inner.base
    }

    /// Returns the server-specific configuration for this app.
    pub fn settings(&self) -> FirebaseServerAppSettings {
        self.inner.settings.clone()
    }

    /// Convenience accessor for the app name.
    pub fn name(&self) -> &str {
        self.inner.base.name()
    }

    /// Increments the manual reference count.
    pub fn inc_ref_count(&self) {
        self.inner.ref_count.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrements the reference count, returning the new value.
    pub fn dec_ref_count(&self) -> usize {
        self.inner.ref_count.fetch_sub(1, Ordering::SeqCst) - 1
    }

    /// Enables automatic cleanup when the server app is dropped.
    pub fn set_release_on_drop(&self, enabled: bool) {
        self.inner.release_on_drop.store(enabled, Ordering::SeqCst);
    }

    /// Indicates whether automatic cleanup is currently configured.
    pub fn release_on_drop(&self) -> bool {
        self.inner.release_on_drop.load(Ordering::SeqCst)
    }
}

/// Returns `true` when the current target behaves like a browser environment.
pub fn is_browser() -> bool {
    environment::is_browser()
}

/// Returns `true` when the current target is a web worker environment.
pub fn is_web_worker() -> bool {
    environment::is_web_worker()
}

/// Provides default app options sourced from environment configuration when available.
pub fn get_default_app_config() -> Option<FirebaseOptions> {
    let map = environment::default_app_config_json()?;
    map_to_options(&map)
}

/// Compares two `FirebaseOptions` instances for structural equality.
pub fn deep_equal_options(a: &FirebaseOptions, b: &FirebaseOptions) -> bool {
    a == b
}

#[derive(Clone, Debug)]
pub struct FirebaseAuthTokenData {
    pub access_token: String,
}

pub trait FirebaseServiceInternals: Send + Sync {
    fn delete(&self) -> AppResult<()>;
}

#[allow(dead_code)]
pub trait FirebaseService: Send + Sync {
    fn app(&self) -> FirebaseApp;
    fn internals(&self) -> Option<&dyn FirebaseServiceInternals> {
        None
    }
}

#[allow(dead_code)]
pub type AppHook = Arc<dyn Fn(&str, &FirebaseApp) + Send + Sync>;

#[allow(dead_code)]
pub type FirebaseServiceFactory<T> = Arc<
    dyn Fn(&FirebaseApp, Option<Arc<dyn Fn(&HashMap<String, serde_json::Value>) + Send + Sync>>, Option<&str>) -> T
        + Send
        + Sync,
>;

#[allow(dead_code)]
pub type FirebaseServiceNamespace<T> = Arc<dyn Fn(Option<&FirebaseApp>) -> T + Send + Sync>;

#[allow(dead_code)]
pub trait FirebaseAppInternals: Send + Sync {
    fn get_token(&self, refresh_token: bool) -> AppResult<Option<FirebaseAuthTokenData>>;
    fn get_uid(&self) -> Option<String>;
    fn add_auth_token_listener(&self, listener: Arc<dyn Fn(Option<String>) + Send + Sync>);
    fn remove_auth_token_listener(&self, listener_id: usize);
    fn log_event(&self, event_name: &str, event_params: HashMap<String, serde_json::Value>, global: bool);
}

/// Compares two app configs for equality.
pub fn deep_equal_config(a: &FirebaseAppConfig, b: &FirebaseAppConfig) -> bool {
    a == b
}

fn to_arc_str(value: impl Into<String>) -> Arc<str> {
    Arc::from(value.into().into_boxed_str())
}

fn map_to_options(map: &serde_json::Map<String, serde_json::Value>) -> Option<FirebaseOptions> {
    let mut options = FirebaseOptions::default();

    options.api_key = string_value(map, "apiKey");
    options.auth_domain = string_value(map, "authDomain");
    options.database_url = string_value(map, "databaseURL");
    options.project_id = string_value(map, "projectId");
    options.storage_bucket = string_value(map, "storageBucket");
    options.messaging_sender_id = string_value(map, "messagingSenderId");
    options.app_id = string_value(map, "appId");
    options.measurement_id = string_value(map, "measurementId");

    if options.api_key.is_some()
        || options.project_id.is_some()
        || options.app_id.is_some()
        || options.database_url.is_some()
        || options.storage_bucket.is_some()
        || options.messaging_sender_id.is_some()
        || options.measurement_id.is_some()
        || options.auth_domain.is_some()
    {
        Some(options)
    } else {
        None
    }
}

fn string_value(map: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

impl Drop for FirebaseServerApp {
    fn drop(&mut self) {
        if !self.release_on_drop() {
            return;
        }

        if self.base().is_deleted() {
            return;
        }

        let was_enabled = self.inner.release_on_drop.swap(false, Ordering::SeqCst);
        if !was_enabled {
            return;
        }

        let app = self.inner.base.clone();
        spawn_detached(async move {
            let _ = crate::app::api::delete_app(&app).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{LazyLock, Mutex};

    static ENV_GUARD: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    #[test]
    fn map_to_options_returns_some_when_fields_present() {
        let mut map = serde_json::Map::new();
        map.insert("apiKey".into(), serde_json::Value::String("foo".into()));
        let options = map_to_options(&map).expect("options");
        assert_eq!(options.api_key.as_deref(), Some("foo"));
    }

    #[test]
    fn map_to_options_returns_none_for_empty_map() {
        let map = serde_json::Map::new();
        assert!(map_to_options(&map).is_none());
    }

    #[test]
    fn get_default_app_config_reads_environment() {
        let _guard = ENV_GUARD.lock().unwrap();

        let key = "FIREBASE_CONFIG";
        let previous = std::env::var(key).ok();
        std::env::set_var(key, "{\"apiKey\":\"env-key\"}");

        let options = get_default_app_config().expect("config");
        assert_eq!(options.api_key.as_deref(), Some("env-key"));

        match previous {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}
