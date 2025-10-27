#![doc = include_str!("RUSTDOC.md")]

pub mod ai;
pub mod analytics;
pub mod app;
pub mod app_check;
pub mod auth;
pub mod component;

// TODO(async-wasm): Re-enable data_connect once Stage 3 migration lands.
// #[cfg(not(target_arch = "wasm32"))]
// pub mod data_connect;
// #[cfg(target_arch = "wasm32")]
// pub mod data_connect {}

pub mod database;

pub mod firestore;

pub mod functions;

pub mod installations;
pub mod logger;
pub mod messaging;

pub mod performance;
pub mod platform;
pub mod remote_config;

pub mod storage;

pub mod util;

#[cfg(test)]
pub mod test_support;
