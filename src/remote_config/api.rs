use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app;
use crate::app::FirebaseApp;
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType};
use crate::installations::get_installations;
use crate::remote_config::constants::{
    RC_CUSTOM_SIGNAL_KEY_MAX_LENGTH, RC_CUSTOM_SIGNAL_VALUE_MAX_LENGTH, REMOTE_CONFIG_API_URL,
    REMOTE_CONFIG_COMPONENT_NAME,
};
use crate::remote_config::error::{internal_error, invalid_argument, RemoteConfigResult};
#[cfg(not(target_arch = "wasm32"))]
use crate::remote_config::fetch::HttpRemoteConfigFetchClient;
#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
use crate::remote_config::fetch::WasmRemoteConfigFetchClient;
use crate::remote_config::fetch::{
    FetchRequest, InstallationsTokenProvider, NoopFetchClient, RemoteConfigFetchClient,
};
use crate::remote_config::settings::{RemoteConfigSettings, RemoteConfigSettingsUpdate};
pub use crate::remote_config::storage::CustomSignals;
use crate::remote_config::storage::{
    FetchStatus, InMemoryRemoteConfigStorage, RemoteConfigStorage, RemoteConfigStorageCache,
};
use crate::remote_config::value::{RemoteConfigValue, RemoteConfigValueSource};
use async_lock::OnceCell;
#[cfg(not(target_arch = "wasm32"))]
use reqwest::Client as HttpClient;
#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
use reqwest::Client as WasmHttpClient;
use serde_json::Value as JsonValue;

#[derive(Clone)]
pub struct RemoteConfig {
    inner: Arc<RemoteConfigInner>,
}

struct RemoteConfigInner {
    app: FirebaseApp,
    defaults: Mutex<HashMap<String, String>>,
    fetched_config: Mutex<HashMap<String, String>>,
    fetched_etag: Mutex<Option<String>>,
    fetched_template_version: Mutex<Option<u64>>,
    activated: Mutex<bool>,
    settings: Mutex<RemoteConfigSettings>,
    fetch_client: Mutex<Arc<dyn RemoteConfigFetchClient>>,
    storage_cache: RemoteConfigStorageCache,
    initialize_once: OnceCell<()>,
}

impl RemoteConfigInner {
    async fn ensure_initialized(&self) -> RemoteConfigResult<()> {
        self.initialize_once
            .get_or_try_init(|| async {
                self.storage_cache.hydrate_from_storage().await?;
                Ok(())
            })
            .await
            .map(|_| ())
    }
}
static REMOTE_CONFIG_CACHE: LazyLock<Mutex<HashMap<String, Arc<RemoteConfig>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

impl RemoteConfig {
    fn new(app: FirebaseApp) -> Self {
        Self::with_storage(app, Arc::new(InMemoryRemoteConfigStorage::default()))
    }

    pub fn with_storage(app: FirebaseApp, storage: Arc<dyn RemoteConfigStorage>) -> Self {
        let storage_cache = RemoteConfigStorageCache::new(storage);
        let fetch_client: Arc<dyn RemoteConfigFetchClient> = default_fetch_client(&app);

        Self {
            inner: Arc::new(RemoteConfigInner {
                app,
                defaults: Mutex::new(HashMap::new()),
                fetched_config: Mutex::new(HashMap::new()),
                fetched_etag: Mutex::new(None),
                fetched_template_version: Mutex::new(None),
                activated: Mutex::new(false),
                settings: Mutex::new(RemoteConfigSettings::default()),
                fetch_client: Mutex::new(fetch_client),
                storage_cache,
                initialize_once: OnceCell::new(),
            }),
        }
    }

    pub fn app(&self) -> &FirebaseApp {
        &self.inner.app
    }

    pub fn set_defaults(&self, defaults: HashMap<String, String>) {
        *self.inner.defaults.lock().unwrap() = defaults;
    }

    /// Replaces the underlying fetch client.
    ///
    /// Useful for tests or environments that need to supply a custom transport implementation,
    /// such as [`HttpRemoteConfigFetchClient`](crate::remote_config::fetch::HttpRemoteConfigFetchClient).
    pub fn set_fetch_client(&self, fetch_client: Arc<dyn RemoteConfigFetchClient>) {
        *self.inner.fetch_client.lock().unwrap() = fetch_client;
    }

    /// Returns a copy of the current Remote Config settings.
    ///
    /// Mirrors the JS `remoteConfig.settings` property.
    pub fn settings(&self) -> RemoteConfigSettings {
        self.inner.settings.lock().unwrap().clone()
    }

    /// Applies validated settings to the Remote Config instance.
    ///
    /// Equivalent to the JS `setConfigSettings` helper. Values are merged with the existing
    /// configuration and validated before being applied.
    ///
    /// # Examples
    ///
    /// ```
    /// use firebase_rs_sdk::remote_config::settings::RemoteConfigSettingsUpdate;
    /// # use firebase_rs_sdk::remote_config::get_remote_config;
    /// # use firebase_rs_sdk::app::api::initialize_app;
    /// # use firebase_rs_sdk::app::{FirebaseOptions, FirebaseAppSettings};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let app = initialize_app(FirebaseOptions::default(), Some(FirebaseAppSettings::default())).await?;
    /// let rc = get_remote_config(Some(app)).await?;
    /// rc.set_config_settings(RemoteConfigSettingsUpdate {
    ///     fetch_timeout_millis: Some(90_000),
    ///     minimum_fetch_interval_millis: Some(3_600_000),
    /// })?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_config_settings(
        &self,
        update: RemoteConfigSettingsUpdate,
    ) -> RemoteConfigResult<()> {
        if update.is_empty() {
            return Ok(());
        }

        let mut settings = self.inner.settings.lock().unwrap();

        if let Some(fetch_timeout) = update.fetch_timeout_millis {
            settings.set_fetch_timeout_millis(fetch_timeout)?;
        }

        if let Some(min_interval) = update.minimum_fetch_interval_millis {
            settings.set_minimum_fetch_interval_millis(min_interval)?;
        }

        Ok(())
    }

    /// Ensures that cached values and metadata are loaded from the underlying storage backend.
    ///
    /// Mirrors the JS SDK's `ensureInitialized()` helper.
    pub async fn ensure_initialized(&self) -> RemoteConfigResult<()> {
        self.inner.ensure_initialized().await
    }

    pub async fn fetch(&self) -> RemoteConfigResult<()> {
        self.inner.ensure_initialized().await?;
        let now = current_timestamp_millis();
        let settings = self.inner.settings.lock().unwrap().clone();

        if let Some(last_fetch) = self
            .inner
            .storage_cache
            .last_successful_fetch_timestamp_millis()
        {
            let elapsed = now.saturating_sub(last_fetch);
            if settings.minimum_fetch_interval_millis() > 0
                && elapsed < settings.minimum_fetch_interval_millis()
            {
                self.inner
                    .storage_cache
                    .set_last_fetch_status(FetchStatus::Throttle)
                    .await?;
                return Err(invalid_argument(
                    "minimum_fetch_interval_millis has not elapsed since the last successful fetch",
                ));
            }
        }

        let request = FetchRequest {
            cache_max_age_millis: settings.minimum_fetch_interval_millis(),
            timeout_millis: settings.fetch_timeout_millis(),
            e_tag: self.inner.storage_cache.active_config_etag(),
            custom_signals: self.inner.storage_cache.custom_signals(),
        };

        let fetch_client = self.inner.fetch_client.lock().unwrap().clone();
        let response = fetch_client.fetch(request).await;

        let response = match response {
            Ok(res) => res,
            Err(err) => {
                self.inner
                    .storage_cache
                    .set_last_fetch_status(FetchStatus::Failure)
                    .await?;
                return Err(err);
            }
        };

        match response.status {
            200 => {
                let config = response.config.unwrap_or_default();
                let etag = response.etag;
                {
                    let mut fetched = self.inner.fetched_config.lock().unwrap();
                    *fetched = config;
                }
                {
                    let mut fetched_etag = self.inner.fetched_etag.lock().unwrap();
                    *fetched_etag = etag;
                }
                {
                    let mut fetched_template_version =
                        self.inner.fetched_template_version.lock().unwrap();
                    *fetched_template_version = response.template_version;
                }
                *self.inner.activated.lock().unwrap() = false;
                self.inner
                    .storage_cache
                    .set_last_fetch_status(FetchStatus::Success)
                    .await?;
                self.inner
                    .storage_cache
                    .set_last_successful_fetch_timestamp_millis(now)
                    .await?;
                Ok(())
            }
            304 => {
                self.inner
                    .storage_cache
                    .set_last_fetch_status(FetchStatus::Success)
                    .await?;
                self.inner
                    .storage_cache
                    .set_last_successful_fetch_timestamp_millis(now)
                    .await?;
                Ok(())
            }
            status => {
                self.inner
                    .storage_cache
                    .set_last_fetch_status(FetchStatus::Failure)
                    .await?;
                Err(internal_error(format!(
                    "fetch returned unexpected status {}",
                    status
                )))
            }
        }
    }

    pub async fn activate(&self) -> RemoteConfigResult<bool> {
        self.inner.ensure_initialized().await?;
        let mut activated = self.inner.activated.lock().unwrap();
        let changed = !*activated;
        if changed {
            let mut fetched = self.inner.fetched_config.lock().unwrap();
            let config = if fetched.is_empty() {
                self.inner.defaults.lock().unwrap().clone()
            } else {
                fetched.clone()
            };
            fetched.clear();
            drop(fetched);

            let mut fetched_etag = self.inner.fetched_etag.lock().unwrap();
            let etag = fetched_etag.take();
            drop(fetched_etag);

            let mut fetched_template_version = self.inner.fetched_template_version.lock().unwrap();
            let template_version = fetched_template_version.take();
            drop(fetched_template_version);

            self.inner.storage_cache.set_active_config(config).await?;
            self.inner
                .storage_cache
                .set_active_config_etag(etag)
                .await?;
            self.inner
                .storage_cache
                .set_active_config_template_version(template_version)
                .await?;
        }
        *activated = true;
        Ok(changed)
    }

    /// Returns the timestamp (in milliseconds since epoch) of the last successful fetch.
    ///
    /// Mirrors `remoteConfig.fetchTimeMillis` from the JS SDK, returning `-1` when no successful
    /// fetch has completed yet.
    pub fn fetch_time_millis(&self) -> i64 {
        self.inner
            .storage_cache
            .last_successful_fetch_timestamp_millis()
            .map(|millis| millis as i64)
            .unwrap_or(-1)
    }

    /// Returns the status of the last fetch attempt.
    ///
    /// Matches the JS `remoteConfig.lastFetchStatus` property.
    pub fn last_fetch_status(&self) -> FetchStatus {
        self.inner.storage_cache.last_fetch_status()
    }

    /// Returns the template version of the currently active Remote Config, if known.
    pub fn active_template_version(&self) -> Option<u64> {
        self.inner.storage_cache.active_config_template_version()
    }

    /// Returns the raw string value for a parameter.
    ///
    /// Mirrors the JS helper `getString` defined in `packages/remote-config/src/api.ts`.
    pub fn get_string(&self, key: &str) -> String {
        self.get_value(key).as_string()
    }

    /// Returns the value interpreted as a boolean.
    ///
    /// Maps to the JS helper `getBoolean`.
    pub fn get_boolean(&self, key: &str) -> bool {
        self.get_value(key).as_bool()
    }

    /// Returns the value interpreted as a number.
    ///
    /// Maps to the JS helper `getNumber`.
    pub fn get_number(&self, key: &str) -> f64 {
        self.get_value(key).as_number()
    }

    /// Returns a value wrapper that exposes typed accessors and the source of the parameter.
    pub fn get_value(&self, key: &str) -> RemoteConfigValue {
        if let Some(value) = self.inner.storage_cache.active_config().get(key).cloned() {
            return RemoteConfigValue::new(RemoteConfigValueSource::Remote, value);
        }
        if let Some(value) = self.inner.defaults.lock().unwrap().get(key).cloned() {
            return RemoteConfigValue::new(RemoteConfigValueSource::Default, value);
        }
        RemoteConfigValue::static_value()
    }

    /// Returns the union of default and active configs, with active values taking precedence.
    pub fn get_all(&self) -> HashMap<String, RemoteConfigValue> {
        let defaults = self.inner.defaults.lock().unwrap().clone();
        let values = self.inner.storage_cache.active_config();

        let mut all = HashMap::new();
        for (key, value) in defaults {
            all.insert(
                key,
                RemoteConfigValue::new(RemoteConfigValueSource::Default, value),
            );
        }
        for (key, value) in values {
            all.insert(
                key,
                RemoteConfigValue::new(RemoteConfigValueSource::Remote, value),
            );
        }
        all
    }

    /// Returns the currently configured custom signals, if any.
    pub fn custom_signals(&self) -> Option<CustomSignals> {
        self.inner.storage_cache.custom_signals()
    }

    /// Merges the provided custom signals into the stored map.
    ///
    /// Passing `serde_json::Value::Null` for a key removes the stored value,
    /// mirroring the JS SDK behaviour. Keys and values are validated to match
    /// the Remote Config limits.
    pub async fn set_custom_signals(&self, signals: CustomSignals) -> RemoteConfigResult<()> {
        if signals.is_empty() {
            self.inner.ensure_initialized().await?;
            return Ok(());
        }

        validate_custom_signals(&signals)?;
        self.inner.ensure_initialized().await?;
        let _ = self.inner.storage_cache.set_custom_signals(signals).await?;
        Ok(())
    }

    /// Fetches the latest Remote Config template and activates it if changes were returned.
    pub async fn fetch_and_activate(&self) -> RemoteConfigResult<bool> {
        self.fetch().await?;
        self.activate().await
    }
}

fn validate_custom_signals(signals: &CustomSignals) -> RemoteConfigResult<()> {
    for (key, value) in signals {
        if key.len() > RC_CUSTOM_SIGNAL_KEY_MAX_LENGTH {
            return Err(invalid_argument(format!(
                "custom signal key '{key}' exceeds {RC_CUSTOM_SIGNAL_KEY_MAX_LENGTH} characters"
            )));
        }

        match value {
            JsonValue::Null | JsonValue::Bool(_) => {}
            JsonValue::Number(number) => {
                if number.to_string().len() > RC_CUSTOM_SIGNAL_VALUE_MAX_LENGTH {
                    return Err(invalid_argument(format!(
                        "custom signal '{key}' stringified value exceeds {} characters",
                        RC_CUSTOM_SIGNAL_VALUE_MAX_LENGTH
                    )));
                }
            }
            JsonValue::String(text) => {
                if text.len() > RC_CUSTOM_SIGNAL_VALUE_MAX_LENGTH {
                    return Err(invalid_argument(format!(
                        "custom signal '{key}' value exceeds {} characters",
                        RC_CUSTOM_SIGNAL_VALUE_MAX_LENGTH
                    )));
                }
            }
            _ => {
                return Err(invalid_argument(format!(
                    "custom signal '{key}' must be null, bool, number, or string"
                )));
            }
        }
    }

    Ok(())
}

fn default_fetch_client(app: &FirebaseApp) -> Arc<dyn RemoteConfigFetchClient> {
    match build_fetch_client(app) {
        Ok(client) => client,
        Err(err) => {
            crate::app::LOGGER.warn(format!(
                "remote-config: falling back to noop fetch client: {}",
                err
            ));
            Arc::new(NoopFetchClient::default())
        }
    }
}

fn build_fetch_client(app: &FirebaseApp) -> RemoteConfigResult<Arc<dyn RemoteConfigFetchClient>> {
    let options = app.options();

    let api_key = options
        .api_key
        .clone()
        .ok_or_else(|| internal_error("Remote Config requires apiKey in FirebaseOptions"))?;
    let project_id = options
        .project_id
        .clone()
        .ok_or_else(|| internal_error("Remote Config requires projectId in FirebaseOptions"))?;
    let app_id = options
        .app_id
        .clone()
        .ok_or_else(|| internal_error("Remote Config requires appId in FirebaseOptions"))?;
    let namespace = project_id.clone();
    let language_code = std::env::var("FIREBASE_REMOTE_CONFIG_LANGUAGE_CODE")
        .unwrap_or_else(|_| "en-US".to_string());
    let sdk_version = format!("w:{}", crate::app::SDK_VERSION);
    let base_url = std::env::var("FIREBASE_REMOTE_CONFIG_API_URL")
        .unwrap_or_else(|_| REMOTE_CONFIG_API_URL.to_string());

    let installations =
        get_installations(Some(app.clone())).map_err(|err| internal_error(err.to_string()))?;

    #[cfg(not(target_arch = "wasm32"))]
    {
        let installations: Arc<dyn InstallationsTokenProvider> = installations;
        let client = HttpClient::builder()
            .user_agent(format!("firebase-rs-sdk/{}", crate::app::SDK_VERSION))
            .build()
            .map_err(|err| internal_error(format!("Failed to build HTTP client: {err}")))?;
        let fetch = HttpRemoteConfigFetchClient::new(
            client,
            &base_url,
            project_id,
            namespace,
            api_key,
            app_id,
            sdk_version,
            language_code,
            installations,
        );
        return Ok(Arc::new(fetch));
    }

    #[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
    {
        let installations: Arc<dyn InstallationsTokenProvider> = installations;
        let client = WasmHttpClient::new();
        let fetch = WasmRemoteConfigFetchClient::new(
            client,
            &base_url,
            project_id,
            namespace,
            api_key,
            app_id,
            sdk_version,
            language_code,
            installations,
        );
        return Ok(Arc::new(fetch));
    }

    #[cfg(all(target_arch = "wasm32", not(feature = "wasm-web")))]
    {
        let _ = installations;
        return Err(internal_error(
            "Building Remote Config for wasm32 requires the `wasm-web` feature",
        ));
    }
}

impl fmt::Debug for RemoteConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let defaults_len = self.inner.defaults.lock().map(|map| map.len()).unwrap_or(0);
        f.debug_struct("RemoteConfig")
            .field("app", &self.app().name())
            .field("defaults", &defaults_len)
            .field("last_fetch_status", &self.last_fetch_status().as_str())
            .finish()
    }
}

static REMOTE_CONFIG_COMPONENT: LazyLock<()> = LazyLock::new(|| {
    let component = Component::new(
        REMOTE_CONFIG_COMPONENT_NAME,
        Arc::new(remote_config_factory),
        ComponentType::Public,
    )
    .with_instantiation_mode(InstantiationMode::Lazy);
    let _ = app::registry::register_component(component);
});

fn remote_config_factory(
    container: &crate::component::ComponentContainer,
    _options: InstanceFactoryOptions,
) -> Result<DynService, ComponentError> {
    let app = container.root_service::<FirebaseApp>().ok_or_else(|| {
        ComponentError::InitializationFailed {
            name: REMOTE_CONFIG_COMPONENT_NAME.to_string(),
            reason: "Firebase app not attached to component container".to_string(),
        }
    })?;

    let rc = RemoteConfig::new((*app).clone());
    Ok(Arc::new(rc) as DynService)
}

fn current_timestamp_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn ensure_registered() {
    LazyLock::force(&REMOTE_CONFIG_COMPONENT);
}

pub fn register_remote_config_component() {
    ensure_registered();
}

pub async fn get_remote_config(app: Option<FirebaseApp>) -> RemoteConfigResult<Arc<RemoteConfig>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => crate::app::api::get_app(None)
            .await
            .map_err(|err| internal_error(err.to_string()))?,
    };

    if let Some(rc) = REMOTE_CONFIG_CACHE.lock().unwrap().get(app.name()).cloned() {
        return Ok(rc);
    }

    let provider = app::registry::get_provider(&app, REMOTE_CONFIG_COMPONENT_NAME);
    if let Some(rc) = provider.get_immediate::<RemoteConfig>() {
        REMOTE_CONFIG_CACHE
            .lock()
            .unwrap()
            .insert(app.name().to_string(), rc.clone());
        return Ok(rc);
    }

    match provider.initialize::<RemoteConfig>(serde_json::Value::Null, None) {
        Ok(rc) => {
            REMOTE_CONFIG_CACHE
                .lock()
                .unwrap()
                .insert(app.name().to_string(), rc.clone());
            Ok(rc)
        }
        Err(crate::component::types::ComponentError::InstanceUnavailable { .. }) => {
            if let Some(rc) = provider.get_immediate::<RemoteConfig>() {
                REMOTE_CONFIG_CACHE
                    .lock()
                    .unwrap()
                    .insert(app.name().to_string(), rc.clone());
                Ok(rc)
            } else {
                let rc = Arc::new(RemoteConfig::new(app.clone()));
                REMOTE_CONFIG_CACHE
                    .lock()
                    .unwrap()
                    .insert(app.name().to_string(), rc.clone());
                Ok(rc)
            }
        }
        Err(err) => Err(internal_error(err.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::api::initialize_app;
    use crate::app::{FirebaseApp, FirebaseAppSettings, FirebaseOptions};
    use crate::remote_config::constants::{
        RC_CUSTOM_SIGNAL_KEY_MAX_LENGTH, RC_CUSTOM_SIGNAL_VALUE_MAX_LENGTH,
    };
    use crate::remote_config::error::{internal_error, RemoteConfigErrorCode};
    use crate::remote_config::fetch::{FetchRequest, FetchResponse, RemoteConfigFetchClient};
    use crate::remote_config::settings::{
        RemoteConfigSettingsUpdate, DEFAULT_FETCH_TIMEOUT_MILLIS,
        DEFAULT_MINIMUM_FETCH_INTERVAL_MILLIS,
    };
    #[cfg(not(target_arch = "wasm32"))]
    use crate::remote_config::storage::FileRemoteConfigStorage;
    use crate::remote_config::storage::{CustomSignals, FetchStatus, RemoteConfigStorage};
    use serde_json::{json, Value as JsonValue};
    use std::fs;
    use std::future::Future;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex as StdMutex;
    use tokio::runtime::Builder;

    fn block_on_future<F: Future>(future: F) -> F::Output
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    fn run_fetch(rc: &RemoteConfig) -> RemoteConfigResult<()> {
        let rc_clone = rc.clone();
        block_on_future(async move { rc_clone.fetch().await })
    }

    fn run_activate(rc: &RemoteConfig) -> RemoteConfigResult<bool> {
        let rc_clone = rc.clone();
        block_on_future(async move { rc_clone.activate().await })
    }

    fn run_ensure_initialized(rc: &RemoteConfig) -> RemoteConfigResult<()> {
        let rc_clone = rc.clone();
        block_on_future(async move { rc_clone.ensure_initialized().await })
    }

    fn run_set_custom_signals(rc: &RemoteConfig, signals: CustomSignals) -> RemoteConfigResult<()> {
        let rc_clone = rc.clone();
        block_on_future(async move { rc_clone.set_custom_signals(signals).await })
    }

    fn remote_config(app: FirebaseApp) -> Arc<RemoteConfig> {
        Arc::new(RemoteConfig::new(app))
    }

    fn unique_settings() -> FirebaseAppSettings {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!(
                "remote-config-{}",
                COUNTER.fetch_add(1, Ordering::SeqCst)
            )),
            ..Default::default()
        }
    }

    #[test]
    fn defaults_activate() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = block_on_future(initialize_app(options, Some(unique_settings()))).unwrap();
        let rc = remote_config(app);
        rc.set_defaults(HashMap::from([(
            String::from("welcome"),
            String::from("hello"),
        )]));
        run_fetch(&rc).unwrap();
        assert!(run_activate(&rc).unwrap());
        assert_eq!(rc.get_string("welcome"), "hello");
        assert_eq!(rc.last_fetch_status(), FetchStatus::Success);
        assert!(rc.fetch_time_millis() > 0);
    }

    #[test]
    fn activate_after_defaults_returns_false() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = block_on_future(initialize_app(options, Some(unique_settings()))).unwrap();
        let rc = remote_config(app);
        rc.set_defaults(HashMap::from([(String::from("flag"), String::from("off"))]));
        run_fetch(&rc).unwrap();
        run_activate(&rc).unwrap();
        assert!(!run_activate(&rc).unwrap());
    }

    #[test]
    fn get_value_reports_default_source_prior_to_activation() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = block_on_future(initialize_app(options, Some(unique_settings()))).unwrap();
        let rc = remote_config(app);
        rc.set_defaults(HashMap::from([(
            String::from("feature"),
            String::from("true"),
        )]));

        let value = rc.get_value("feature");
        assert_eq!(value.source(), RemoteConfigValueSource::Default);
        assert!(value.as_bool());
    }

    #[test]
    fn get_value_reports_remote_source_after_activation() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = block_on_future(initialize_app(options, Some(unique_settings()))).unwrap();
        let rc = remote_config(app);
        rc.set_defaults(HashMap::from([(
            String::from("feature"),
            String::from("true"),
        )]));
        run_fetch(&rc).unwrap();
        run_activate(&rc).unwrap();

        let value = rc.get_value("feature");
        assert_eq!(value.source(), RemoteConfigValueSource::Remote);
        assert!(value.as_bool());
    }

    #[test]
    fn get_number_handles_invalid_values() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = block_on_future(initialize_app(options, Some(unique_settings()))).unwrap();
        let rc = remote_config(app);
        rc.set_defaults(HashMap::from([(
            String::from("rate"),
            String::from("not-a-number"),
        )]));

        assert_eq!(rc.get_number("rate"), 0.0);
        assert_eq!(rc.get_number("missing"), 0.0);
    }

    #[test]
    fn get_all_merges_defaults_and_remote_values() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = block_on_future(initialize_app(options, Some(unique_settings()))).unwrap();
        let rc = remote_config(app);
        rc.set_defaults(HashMap::from([
            (String::from("feature"), String::from("true")),
            (String::from("secondary"), String::from("value")),
        ]));
        run_fetch(&rc).unwrap();
        run_activate(&rc).unwrap();
        rc.set_defaults(HashMap::from([
            (String::from("feature"), String::from("false")),
            (String::from("secondary"), String::from("value")),
            (String::from("fallback"), String::from("present")),
        ]));

        let all = rc.get_all();
        assert_eq!(all.len(), 3);
        assert_eq!(all["feature"].source(), RemoteConfigValueSource::Remote);
        assert_eq!(all["feature"].as_bool(), true);
        assert_eq!(all["secondary"].source(), RemoteConfigValueSource::Remote);
        assert_eq!(all["fallback"].source(), RemoteConfigValueSource::Default);
    }

    #[test]
    fn missing_key_returns_static_value() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = block_on_future(initialize_app(options, Some(unique_settings()))).unwrap();
        let rc = remote_config(app);

        let value = rc.get_value("not-present");
        assert_eq!(value.source(), RemoteConfigValueSource::Static);
        assert_eq!(value.as_string(), "");
        assert!(!value.as_bool());
        assert_eq!(value.as_number(), 0.0);
    }

    #[test]
    fn settings_defaults_match_js_constants() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = block_on_future(initialize_app(options, Some(unique_settings()))).unwrap();
        let rc = remote_config(app);

        let settings = rc.settings();
        assert_eq!(
            settings.fetch_timeout_millis(),
            DEFAULT_FETCH_TIMEOUT_MILLIS
        );
        assert_eq!(
            settings.minimum_fetch_interval_millis(),
            DEFAULT_MINIMUM_FETCH_INTERVAL_MILLIS
        );
    }

    #[test]
    fn set_config_settings_updates_values() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = block_on_future(initialize_app(options, Some(unique_settings()))).unwrap();
        let rc = remote_config(app);

        rc.set_config_settings(RemoteConfigSettingsUpdate {
            fetch_timeout_millis: Some(90_000),
            minimum_fetch_interval_millis: Some(3_600_000),
        })
        .unwrap();

        let settings = rc.settings();
        assert_eq!(settings.fetch_timeout_millis(), 90_000);
        assert_eq!(settings.minimum_fetch_interval_millis(), 3_600_000);
    }

    #[test]
    fn set_config_settings_rejects_zero_timeout() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = block_on_future(initialize_app(options, Some(unique_settings()))).unwrap();
        let rc = remote_config(app);

        let result = rc.set_config_settings(RemoteConfigSettingsUpdate {
            fetch_timeout_millis: Some(0),
            minimum_fetch_interval_millis: None,
        });

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code_str(),
            crate::remote_config::error::RemoteConfigErrorCode::InvalidArgument.as_str()
        );
    }

    #[test]
    fn fetch_metadata_defaults() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = block_on_future(initialize_app(options, Some(unique_settings()))).unwrap();
        let rc = remote_config(app);

        assert_eq!(rc.last_fetch_status(), FetchStatus::NoFetchYet);
        assert_eq!(rc.fetch_time_millis(), -1);
    }

    #[test]
    fn fetch_respects_minimum_fetch_interval() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = block_on_future(initialize_app(options, Some(unique_settings()))).unwrap();
        let rc = remote_config(app);

        run_fetch(&rc).unwrap();
        let result = run_fetch(&rc);

        assert!(result.is_err());
        assert_eq!(rc.last_fetch_status(), FetchStatus::Throttle);
    }

    #[test]
    fn fetch_and_activate_uses_remote_values() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = block_on_future(initialize_app(options, Some(unique_settings()))).unwrap();
        let rc = remote_config(app);

        let response = FetchResponse {
            status: 200,
            etag: Some(String::from("etag-1")),
            config: Some(HashMap::from([(
                String::from("feature"),
                String::from("remote"),
            )])),
            template_version: Some(7),
        };

        rc.set_fetch_client(Arc::new(StubFetchClient::new(response)));

        run_fetch(&rc).unwrap();
        assert_eq!(rc.last_fetch_status(), FetchStatus::Success);

        assert!(run_activate(&rc).unwrap());
        let value = rc.get_value("feature");
        assert_eq!(value.source(), RemoteConfigValueSource::Remote);
        assert_eq!(value.as_string(), "remote");
        assert_eq!(rc.active_template_version(), Some(7));
    }

    #[test]
    fn set_custom_signals_merges_and_removes() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = block_on_future(initialize_app(options, Some(unique_settings()))).unwrap();
        let rc = remote_config(app);

        let mut first: CustomSignals = HashMap::new();
        first.insert(String::from("flag"), json!(true));
        run_set_custom_signals(&rc, first).unwrap();

        let mut second: CustomSignals = HashMap::new();
        second.insert(String::from("score"), json!(42));
        run_set_custom_signals(&rc, second).unwrap();

        let mut removal: CustomSignals = HashMap::new();
        removal.insert(String::from("flag"), JsonValue::Null);
        run_set_custom_signals(&rc, removal).unwrap();

        let signals = rc.custom_signals().expect("signals stored");
        assert_eq!(
            signals.get("score"),
            Some(&JsonValue::String(String::from("42")))
        );
        assert!(!signals.contains_key("flag"));
    }

    #[test]
    fn set_custom_signals_rejects_too_long_key() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = block_on_future(initialize_app(options, Some(unique_settings()))).unwrap();
        let rc = remote_config(app);

        let mut invalid: CustomSignals = HashMap::new();
        invalid.insert("x".repeat(RC_CUSTOM_SIGNAL_KEY_MAX_LENGTH + 1), json!(true));

        let err = run_set_custom_signals(&rc, invalid).unwrap_err();
        assert_eq!(
            err.code_str(),
            RemoteConfigErrorCode::InvalidArgument.as_str()
        );
    }

    #[test]
    fn set_custom_signals_rejects_too_long_value() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = block_on_future(initialize_app(options, Some(unique_settings()))).unwrap();
        let rc = remote_config(app);

        let mut invalid: CustomSignals = HashMap::new();
        invalid.insert(
            String::from("flag"),
            JsonValue::String("x".repeat(RC_CUSTOM_SIGNAL_VALUE_MAX_LENGTH + 1)),
        );

        let err = run_set_custom_signals(&rc, invalid).unwrap_err();
        assert_eq!(
            err.code_str(),
            RemoteConfigErrorCode::InvalidArgument.as_str()
        );
    }

    #[test]
    fn fetch_includes_custom_signals_in_request() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = block_on_future(initialize_app(options, Some(unique_settings()))).unwrap();
        let rc = remote_config(app);

        let mut signals: CustomSignals = HashMap::new();
        signals.insert(String::from("experiment"), json!("A"));
        signals.insert(String::from("variant"), json!(1));
        run_set_custom_signals(&rc, signals).unwrap();

        let response = FetchResponse {
            status: 200,
            etag: None,
            config: Some(HashMap::new()),
            template_version: None,
        };
        let recording = Arc::new(RecordingFetchClient::new(response));
        rc.set_fetch_client(recording.clone());

        run_fetch(&rc).unwrap();

        let request = recording.last_request().expect("fetch request recorded");
        let sent_signals = request.custom_signals.expect("custom signals included");
        assert_eq!(sent_signals.get("experiment"), Some(&json!("A")));
        assert_eq!(sent_signals.get("variant"), Some(&json!("1")));
    }

    struct StubFetchClient {
        response: StdMutex<Option<FetchResponse>>,
    }

    impl StubFetchClient {
        fn new(response: FetchResponse) -> Self {
            Self {
                response: StdMutex::new(Some(response)),
            }
        }
    }

    struct RecordingFetchClient {
        response: FetchResponse,
        request: StdMutex<Option<FetchRequest>>,
    }

    impl RecordingFetchClient {
        fn new(response: FetchResponse) -> Self {
            Self {
                response,
                request: StdMutex::new(None),
            }
        }

        fn last_request(&self) -> Option<FetchRequest> {
            self.request.lock().unwrap().clone()
        }
    }

    #[cfg_attr(
        all(feature = "wasm-web", target_arch = "wasm32"),
        async_trait::async_trait(?Send)
    )]
    #[cfg_attr(
        not(all(feature = "wasm-web", target_arch = "wasm32")),
        async_trait::async_trait
    )]
    impl RemoteConfigFetchClient for StubFetchClient {
        async fn fetch(&self, _request: FetchRequest) -> RemoteConfigResult<FetchResponse> {
            self.response
                .lock()
                .unwrap()
                .take()
                .ok_or_else(|| internal_error("no response queued"))
        }
    }

    #[cfg_attr(
        all(feature = "wasm-web", target_arch = "wasm32"),
        async_trait::async_trait(?Send)
    )]
    #[cfg_attr(
        not(all(feature = "wasm-web", target_arch = "wasm32")),
        async_trait::async_trait
    )]
    impl RemoteConfigFetchClient for RecordingFetchClient {
        async fn fetch(&self, request: FetchRequest) -> RemoteConfigResult<FetchResponse> {
            *self.request.lock().unwrap() = Some(request.clone());
            Ok(self.response.clone())
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn with_storage_persists_across_instances() {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let storage_path = std::env::temp_dir().join(format!(
            "firebase-remote-config-api-storage-{}.json",
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));

        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = block_on_future(initialize_app(options, Some(unique_settings()))).unwrap();

        let storage: Arc<dyn RemoteConfigStorage> =
            Arc::new(FileRemoteConfigStorage::new(storage_path.clone()).unwrap());
        let rc = RemoteConfig::with_storage(app.clone(), storage.clone());

        rc.set_fetch_client(Arc::new(StubFetchClient::new(FetchResponse {
            status: 200,
            etag: Some(String::from("persist-etag")),
            config: Some(HashMap::from([(
                String::from("motd"),
                String::from("hello"),
            )])),
            template_version: Some(5),
        })));

        run_fetch(&rc).unwrap();
        run_activate(&rc).unwrap();

        drop(rc);

        let storage2: Arc<dyn RemoteConfigStorage> =
            Arc::new(FileRemoteConfigStorage::new(storage_path.clone()).unwrap());
        let rc2 = RemoteConfig::with_storage(app, storage2);

        run_ensure_initialized(&rc2).unwrap();

        let value = rc2.get_value("motd");
        assert_eq!(value.source(), RemoteConfigValueSource::Remote);
        assert_eq!(value.as_string(), "hello");
        assert_eq!(rc2.active_template_version(), Some(5));

        let _ = fs::remove_file(storage_path);
    }
}
