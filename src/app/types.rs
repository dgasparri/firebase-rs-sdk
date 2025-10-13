use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::app::errors::{AppError, AppResult};
use crate::component::constants::DEFAULT_ENTRY_NAME;
use crate::component::types::DynService;
use crate::component::{Component, ComponentContainer};

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

pub trait HeartbeatService: Send + Sync {
    fn trigger_heartbeat(&self) -> AppResult<()>;
    #[allow(dead_code)]
    fn heartbeats_header(&self) -> AppResult<Option<String>>;
}

pub trait HeartbeatStorage: Send + Sync {
    fn read(&self) -> AppResult<HeartbeatsInStorage>;
    fn overwrite(&self, value: &HeartbeatsInStorage) -> AppResult<()>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeartbeatsInStorage {
    pub last_sent_heartbeat_date: Option<String>,
    pub heartbeats: Vec<SingleDateHeartbeat>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirebaseAppSettings {
    pub name: Option<String>,
    pub automatic_data_collection_enabled: Option<bool>,
}

impl Default for FirebaseAppSettings {
    fn default() -> Self {
        Self {
            name: None,
            automatic_data_collection_enabled: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirebaseAppConfig {
    pub name: Arc<str>,
    pub automatic_data_collection_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirebaseServerAppSettings {
    pub automatic_data_collection_enabled: Option<bool>,
    pub auth_id_token: Option<String>,
    pub app_check_token: Option<String>,
    pub release_on_deref: Option<bool>,
}

impl Default for FirebaseServerAppSettings {
    fn default() -> Self {
        Self {
            automatic_data_collection_enabled: None,
            auth_id_token: None,
            app_check_token: None,
            release_on_deref: None,
        }
    }
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
    pub fn new(
        options: FirebaseOptions,
        config: FirebaseAppConfig,
        container: ComponentContainer,
    ) -> Self {
        let automatic = config.automatic_data_collection_enabled;
        let inner = Arc::new(FirebaseAppInner {
            options,
            config,
            automatic_data_collection_enabled: Mutex::new(automatic),
            is_deleted: AtomicBool::new(false),
            container,
        });
        let app = Self {
            inner: inner.clone(),
        };
        let dyn_service: DynService = Arc::new(app.clone());
        app.inner.container.attach_root_service(dyn_service);
        app
    }

    pub fn name(&self) -> &str {
        &self.inner.config.name
    }

    pub fn options(&self) -> FirebaseOptions {
        self.inner.options.clone()
    }

    pub fn config(&self) -> FirebaseAppConfig {
        self.inner.config.clone()
    }

    pub fn automatic_data_collection_enabled(&self) -> bool {
        *self.inner.automatic_data_collection_enabled.lock().unwrap()
    }

    pub fn set_automatic_data_collection_enabled(&self, value: bool) {
        *self.inner.automatic_data_collection_enabled.lock().unwrap() = value;
    }

    pub fn container(&self) -> ComponentContainer {
        self.inner.container.clone()
    }

    pub fn add_component(&self, component: Component) -> AppResult<()> {
        self.check_destroyed()?;
        self.inner
            .container
            .add_component(component)
            .map_err(AppError::from)
    }

    pub fn add_or_overwrite_component(&self, component: Component) -> AppResult<()> {
        self.check_destroyed()?;
        self.inner.container.add_or_overwrite_component(component);
        Ok(())
    }

    pub fn remove_service_instance(&self, name: &str, identifier: Option<&str>) {
        let provider = self.inner.container.get_provider(name);
        if let Some(id) = identifier {
            provider.clear_instance(id);
        } else {
            provider.clear_instance(DEFAULT_ENTRY_NAME);
        }
    }

    pub fn is_deleted(&self) -> bool {
        self.inner.is_deleted.load(Ordering::SeqCst)
    }

    pub fn set_is_deleted(&self, value: bool) {
        self.inner.is_deleted.store(value, Ordering::SeqCst);
    }

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
            .field(
                "automatic_data_collection_enabled",
                &self.automatic_data_collection_enabled(),
            )
            .finish()
    }
}

impl FirebaseAppConfig {
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
}

impl FirebaseServerApp {
    pub fn new(base: FirebaseApp, settings: FirebaseServerAppSettings) -> Self {
        Self {
            inner: Arc::new(FirebaseServerAppInner {
                base,
                settings,
                ref_count: AtomicUsize::new(1),
            }),
        }
    }

    pub fn base(&self) -> &FirebaseApp {
        &self.inner.base
    }

    pub fn settings(&self) -> FirebaseServerAppSettings {
        self.inner.settings.clone()
    }

    pub fn name(&self) -> &str {
        self.inner.base.name()
    }

    pub fn inc_ref_count(&self) {
        self.inner.ref_count.fetch_add(1, Ordering::SeqCst);
    }

    pub fn dec_ref_count(&self) -> usize {
        self.inner.ref_count.fetch_sub(1, Ordering::SeqCst) - 1
    }
}

#[allow(dead_code)]
pub fn is_browser() -> bool {
    false
}

#[allow(dead_code)]
pub fn is_web_worker() -> bool {
    false
}

pub fn get_default_app_config() -> Option<FirebaseOptions> {
    None
}

pub fn deep_equal_options(a: &FirebaseOptions, b: &FirebaseOptions) -> bool {
    a == b
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct FirebaseAuthTokenData {
    pub access_token: String,
}

#[allow(dead_code)]
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
    dyn Fn(
            &FirebaseApp,
            Option<Arc<dyn Fn(&HashMap<String, serde_json::Value>) + Send + Sync>>,
            Option<&str>,
        ) -> T
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
    fn log_event(
        &self,
        event_name: &str,
        event_params: HashMap<String, serde_json::Value>,
        global: bool,
    );
}

pub fn deep_equal_config(a: &FirebaseAppConfig, b: &FirebaseAppConfig) -> bool {
    a == b
}

fn to_arc_str(value: impl Into<String>) -> Arc<str> {
    Arc::from(value.into().into_boxed_str())
}
