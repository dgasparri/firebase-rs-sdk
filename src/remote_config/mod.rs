#![doc = include_str!("README.md")]
mod api;
mod constants;
pub mod error;
pub mod fetch;
pub mod settings;
pub mod storage;
pub mod value;

pub use api::{get_remote_config, register_remote_config_component, CustomSignals, RemoteConfig};
