#![doc = include_str!("README.md")]
mod api;
pub mod constants;
pub mod error;

pub use api::{
    get_performance, register_performance_component, Performance, PerformanceTrace, TraceHandle,
};
