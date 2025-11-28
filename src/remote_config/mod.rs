#![doc = include_str!("README.md")]
mod api;
mod constants;
mod error;
mod fetch;
mod settings;
mod storage;
mod value;

#[doc(inline)]
pub use api::{get_remote_config, register_remote_config_component, RemoteConfig};

#[doc(inline)]
pub use fetch::{FetchRequest, FetchResponse, InstallationsTokenProvider, NoopFetchClient, RemoteConfigFetchClient};

#[cfg(not(target_arch = "wasm32"))]
#[doc(inline)]
pub use fetch::HttpRemoteConfigFetchClient;

#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
#[doc(inline)]
pub use fetch::WasmRemoteConfigFetchClient;

#[doc(inline)]
pub use error::{internal_error, invalid_argument, RemoteConfigError, RemoteConfigErrorCode, RemoteConfigResult};

#[doc(inline)]
pub use settings::{
    RemoteConfigSettings, RemoteConfigSettingsUpdate, DEFAULT_FETCH_TIMEOUT_MILLIS,
    DEFAULT_MINIMUM_FETCH_INTERVAL_MILLIS,
};

#[allow(unused_imports)]
#[doc(inline)]
pub(crate) use settings::{validate_fetch_timeout, validate_minimum_fetch_interval};

#[doc(inline)]
pub use storage::{
    CustomSignals, FetchStatus, InMemoryRemoteConfigStorage, RemoteConfigStorage, RemoteConfigStorageCache,
};

#[cfg(not(target_arch = "wasm32"))]
#[doc(inline)]
pub use storage::FileRemoteConfigStorage;

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
#[doc(inline)]
pub use storage::IndexedDbRemoteConfigStorage;

#[doc(inline)]
pub use value::{RemoteConfigValue, RemoteConfigValueSource};
