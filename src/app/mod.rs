#![doc = include_str!("README.md")]
mod api;
mod component;
mod constants;
mod core_components;
mod errors;
mod heartbeat;
mod logger;
mod namespace;
mod platform_logger;
mod registry;
mod types;

#[doc(inline)]
pub use api::{
    delete_app, get_app, get_apps, initialize_app, initialize_server_app, on_log, register_version,
    set_log_level, SDK_VERSION,
};

#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use api::clear_registered_versions_for_tests;

#[doc(inline)]
pub use constants::{DEFAULT_ENTRY_NAME, PLATFORM_LOG_STRING};

#[doc(inline)]
pub use core_components::ensure_registered;

#[doc(inline)]
pub use errors::{AppError, AppResult};

#[doc(inline)]
pub use heartbeat::{HeartbeatServiceImpl, InMemoryHeartbeatStorage};

#[cfg(test)]
#[doc(inline)]
pub use heartbeat::clear_heartbeat_store_for_tests;

#[allow(unused_imports)]
pub(crate) use heartbeat::storage_for_app;

#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
#[doc(inline)]
pub use heartbeat::IndexedDbHeartbeatStorage;

// Used in other modules
#[doc(inline)]
pub use logger::{LogCallback, LogLevel, LogOptions, Logger, LOGGER};

#[doc(inline)]
pub use namespace::FirebaseNamespace;

#[doc(inline)]
pub use platform_logger::PlatformLoggerServiceImpl;

// Pub in registry.rs, but mod registry imported as pub(crate) in app mod
#[doc(inline)]
#[allow(unused_imports)]
pub(crate) use registry::{
    add_component, add_or_overwrite_component, clear_components, get_provider, is_firebase_server_app, register_component,
    remove_service_instance, APPS, SERVER_APPS,
};

// pub(crate) also in registry.rs
#[doc(inline)]
#[allow(unused_imports)]
pub(crate) use registry::{apps_guard, registered_components_guard, server_apps_guard};

#[doc(inline)]
pub use types::{
    deep_equal_config, deep_equal_options, get_default_app_config, is_browser, is_web_worker,
    AppHook, FirebaseApp, FirebaseAppConfig, FirebaseAppInternals, FirebaseAppSettings,
    FirebaseAuthTokenData, FirebaseOptions, FirebaseServerApp, FirebaseServerAppSettings,
    FirebaseService, FirebaseServiceFactory, FirebaseServiceInternals, FirebaseServiceNamespace,
    HeartbeatService, HeartbeatStorage, HeartbeatsInStorage, PlatformLoggerService,
    SingleDateHeartbeat, VersionService,
};

use async_lock::OnceCell;

pub(crate) async fn ensure_core_components_registered() {
    CORE_COMPONENTS_REGISTERED
        .get_or_init(core_components::ensure_registered)
        .await;
}

static CORE_COMPONENTS_REGISTERED: OnceCell<()> = OnceCell::new();
