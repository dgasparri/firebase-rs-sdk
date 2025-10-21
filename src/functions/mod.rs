#![doc = include_str!("README.md")]
mod api;
mod constants;
pub mod error;

pub use api::{get_functions, register_functions_component, CallableFunction, Functions};
