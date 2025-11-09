#![doc = include_str!("README.md")]
mod api;
mod constants;
mod error;
mod instrumentation;
mod storage;
mod transport;

#[doc(inline)]
pub use api::{
    get_performance, initialize_performance, is_supported, register_performance_component,
    HttpMethod, NetworkRequestRecord, NetworkTraceHandle, Performance, PerformanceRuntimeSettings,
    PerformanceSettings, PerformanceTrace, TraceHandle, TraceRecordOptions,
};

#[doc(inline)]
pub use constants::{
    MAX_ATTRIBUTE_NAME_LENGTH, MAX_ATTRIBUTE_VALUE_LENGTH, MAX_METRIC_NAME_LENGTH,
    OOB_TRACE_PAGE_LOAD_PREFIX, PERFORMANCE_COMPONENT_NAME, RESERVED_ATTRIBUTE_PREFIXES,
    RESERVED_METRIC_PREFIX,
};

#[doc(inline)]
pub use error::{
    internal_error, invalid_argument, PerformanceError, PerformanceErrorCode, PerformanceResult,
};

// mod instrumentation is private? its functions are used internally only

// mod storage is private? its functions are used internally only

#[doc(inline)]
pub use transport::{TransportController, TransportOptions};
