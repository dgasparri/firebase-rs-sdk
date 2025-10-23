#![doc = include_str!("RUSTDOC.md")]

#[cfg(not(target_arch = "wasm32"))]
pub mod ai;
#[cfg(target_arch = "wasm32")]
pub mod ai {}

#[cfg(not(target_arch = "wasm32"))]
pub mod analytics;
#[cfg(target_arch = "wasm32")]
pub mod analytics {}
pub mod app;
#[cfg(not(target_arch = "wasm32"))]
pub mod app_check;
#[cfg(target_arch = "wasm32")]
pub mod app_check {}
pub mod auth;
pub mod component;
#[cfg(not(target_arch = "wasm32"))]
pub mod data_connect;
#[cfg(target_arch = "wasm32")]
pub mod data_connect {}

#[cfg(not(target_arch = "wasm32"))]
pub mod database;
#[cfg(target_arch = "wasm32")]
pub mod database {}

#[cfg(not(target_arch = "wasm32"))]
pub mod firestore;
#[cfg(target_arch = "wasm32")]
pub mod firestore {}

#[cfg(not(target_arch = "wasm32"))]
pub mod functions;
#[cfg(target_arch = "wasm32")]
pub mod functions {}

#[cfg(not(target_arch = "wasm32"))]
pub mod installations;
#[cfg(target_arch = "wasm32")]
pub mod installations {}
pub mod logger;
#[cfg(not(target_arch = "wasm32"))]
pub mod messaging;
#[cfg(target_arch = "wasm32")]
pub mod messaging {}

#[cfg(not(target_arch = "wasm32"))]
pub mod performance;
#[cfg(target_arch = "wasm32")]
pub mod performance {}
#[cfg(not(target_arch = "wasm32"))]
pub mod platform;
#[cfg(target_arch = "wasm32")]
pub mod platform {}
#[cfg(not(target_arch = "wasm32"))]
pub mod remote_config;
#[cfg(target_arch = "wasm32")]
pub mod remote_config {}

#[cfg(not(target_arch = "wasm32"))]
pub mod storage;
#[cfg(target_arch = "wasm32")]
pub mod storage {}
pub mod util;

#[cfg(test)]
pub mod test_support;
