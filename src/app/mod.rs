#![doc = include_str!("README.md")]
pub mod api;
mod component;
mod constants;
mod core_components;
mod errors;
mod heartbeat;
mod logger;
mod namespace;
mod platform_logger;
pub mod private;
pub(crate) mod registry;
mod types;

#[doc(inline)]
pub use api::{
    delete_app, get_app, get_apps, initialize_app, initialize_server_app, on_log, register_version,
    set_log_level, SDK_VERSION,
};

#[doc(inline)]
pub use errors::{AppError, AppResult};

#[doc(inline)]
pub use logger::{LogCallback, LogLevel, LogOptions, Logger, LOGGER};

#[doc(inline)]
pub use namespace::FirebaseNamespace;

#[doc(inline)]
pub use types::{
    FirebaseApp, FirebaseAppConfig, FirebaseAppSettings, FirebaseOptions, FirebaseServerApp,
    FirebaseServerAppSettings, VersionService,
};

use async_lock::OnceCell;

pub(crate) async fn ensure_core_components_registered() {
    CORE_COMPONENTS_REGISTERED
        .get_or_init(core_components::ensure_registered)
        .await;
}

static CORE_COMPONENTS_REGISTERED: OnceCell<()> = OnceCell::new();
