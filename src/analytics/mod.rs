#![doc = include_str!("README.md")]
mod api;
mod config;
mod constants;
mod error;
mod gtag;
mod transport;

#[doc(inline)]
pub use api::{
    get_analytics, register_analytics_component, Analytics, AnalyticsEvent, AnalyticsSettings, ConsentSettings,
};

#[doc(inline)]
pub use config::DynamicConfig;

#[doc(inline)]
pub use error::{
    config_fetch_failed, internal_error, invalid_argument, missing_measurement_id, network_error, AnalyticsError,
    AnalyticsErrorCode, AnalyticsResult,
};

#[doc(inline)]
pub use gtag::{GlobalGtagRegistry, GtagRegistry, GtagState};

#[doc(inline)]
pub use transport::{MeasurementProtocolConfig, MeasurementProtocolDispatcher, MeasurementProtocolEndpoint};
