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

pub use api::*;
pub use errors::{AppError, AppResult};
pub use logger::{LogCallback, LogLevel, LogOptions, Logger, LOGGER};
pub use namespace::FirebaseNamespace;
pub use types::{
    FirebaseApp, FirebaseAppConfig, FirebaseAppSettings, FirebaseOptions, FirebaseServerApp,
    FirebaseServerAppSettings, VersionService,
};

use std::sync::LazyLock;

pub(crate) fn ensure_core_components_registered() {
    LazyLock::force(&CORE_COMPONENTS_REGISTERED);
}

static CORE_COMPONENTS_REGISTERED: LazyLock<()> = LazyLock::new(|| {
    core_components::ensure_registered();
});
