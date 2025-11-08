#![doc = include_str!("README.md")]
mod api;
pub mod constants;
mod error;

/// Resolves the lazily created [`Performance`](api::Performance) instance for the provided app.
pub use api::get_performance;
/// Explicitly initializes the performance component with custom settings.
pub use api::initialize_performance;
/// Returns whether the current platform supports performance instrumentation.
pub use api::is_supported;
/// Registers the performance component in the shared container (normally invoked automatically).
pub use api::register_performance_component;
/// HTTP method enum reused by [`NetworkTraceHandle`].
pub use api::HttpMethod;
/// Recorded network request metadata, mirroring the JS SDK payload.
pub use api::NetworkRequestRecord;
/// Builder for manual network request instrumentation.
pub use api::NetworkTraceHandle;
/// Primary service façade that mirrors the JS SDK's `FirebasePerformance` controller.
pub use api::Performance;
/// Effective runtime settings reflecting the currently enforced toggles.
pub use api::PerformanceRuntimeSettings;
/// User-supplied configuration toggles applied during initialization.
pub use api::PerformanceSettings;
/// Immutable snapshot of a completed manual trace.
pub use api::PerformanceTrace;
/// Builder/handle for manual traces (the Rust analogue of the JS `Trace`).
pub use api::TraceHandle;
/// Optional metrics/attributes bundle accepted by [`TraceHandle::record`](api::TraceHandle::record).
pub use api::TraceRecordOptions;

pub use error::{PerformanceError, PerformanceResult};