#![doc = include_str!("README.md")]
mod api;
mod constants;
pub mod error;

pub use api::{get_messaging, register_messaging_component, Messaging};
