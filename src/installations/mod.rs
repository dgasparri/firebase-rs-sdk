#![doc = include_str!("README.md")]
mod api;
mod config;
mod constants;
mod error;
mod persistence;
mod rest;
mod types;

pub use api::{
    delete_installations, get_installations, get_installations_internal,
    register_installations_component, IdChangeUnsubscribe, Installations, InstallationsInternal,
};
pub use config::{extract_app_config, AppConfig};

pub use error::{
    internal_error, invalid_argument, request_failed, InstallationsError, InstallationsErrorCode,
    InstallationsResult,
};

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
pub use persistence::FilePersistence;

pub use persistence::{InstallationsPersistence, PersistedAuthToken, PersistedInstallation};

pub use rest::{RegisteredInstallation, INSTALLATIONS_API_URL};
pub use types::{InstallationEntryData, InstallationToken};
