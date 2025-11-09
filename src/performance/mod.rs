#![doc = include_str!("README.md")]
mod api;
mod constants;
mod error;
mod instrumentation;
mod storage;
mod transport;

#[doc(inline)]
pub use api::{
    get_performance,
    initialize_performance,
    is_supported,
    register_performance_component,
    HttpMethod,
    NetworkRequestRecord,
    NetworkTraceHandle,
    Performance,
    PerformanceSettings,
    PerformanceRuntimeSettings,
    PerformanceTrace,
    TraceHandle,
    TraceRecordOptions,
};

#[doc(inline)]
pub use constants::{
    PERFORMANCE_COMPONENT_NAME,
    MAX_ATTRIBUTE_NAME_LENGTH, 
    MAX_ATTRIBUTE_VALUE_LENGTH,
    RESERVED_ATTRIBUTE_PREFIXES,
    MAX_METRIC_NAME_LENGTH,
    RESERVED_METRIC_PREFIX,
    OOB_TRACE_PAGE_LOAD_PREFIX,
};

#[doc(inline)]
pub use error::{invalid_argument, internal_error, PerformanceError, PerformanceErrorCode, PerformanceResult};

// mod instrumentation is private? its functions are used internally only

// mod storage is private? its functions are used internally only

#[doc(inline)]
pub use transport::{TransportOptions, TransportController};


