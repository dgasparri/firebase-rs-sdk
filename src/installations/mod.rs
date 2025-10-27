#![doc = include_str!("README.md")]
mod api;
mod config;
mod constants;
pub mod error;
mod persistence;
mod rest;
mod types;

pub use api::{
    delete_installations, get_installations, get_installations_internal,
    register_installations_component, Installations, InstallationsInternal,
};
pub use config::{extract_app_config, AppConfig};
pub use types::{InstallationEntryData, InstallationToken};
