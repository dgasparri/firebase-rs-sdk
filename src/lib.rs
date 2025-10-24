#![doc = include_str!("RUSTDOC.md")]

// TODO(async-wasm): Re-enable AI module once Stage 3 async migration completes.
// #[cfg(not(target_arch = "wasm32"))]
// pub mod ai;
// #[cfg(target_arch = "wasm32")]
// pub mod ai {}

// TODO(async-wasm): Re-enable analytics module during Stage 3.
// #[cfg(not(target_arch = "wasm32"))]
// pub mod analytics;
// #[cfg(target_arch = "wasm32")]
// pub mod analytics {}
pub mod app;
// TODO(async-wasm): Re-enable app_check when Stage 2 completes the async migration.
// #[cfg(not(target_arch = "wasm32"))]
// pub mod app_check;
// #[cfg(target_arch = "wasm32")]
// pub mod app_check {}
// TODO(async-wasm): Re-enable auth module in Stage 2.
// pub mod auth;
pub mod component;
// TODO(async-wasm): Re-enable data_connect once Stage 3 migration lands.
// #[cfg(not(target_arch = "wasm32"))]
// pub mod data_connect;
// #[cfg(target_arch = "wasm32")]
// pub mod data_connect {}

// TODO(async-wasm): Re-enable database module during Stage 3 async sweep.
// #[cfg(not(target_arch = "wasm32"))]
// pub mod database;
// #[cfg(target_arch = "wasm32")]
// pub mod database {}

// TODO(async-wasm): Re-enable firestore once Stage 3 completes.
// #[cfg(not(target_arch = "wasm32"))]
// pub mod firestore;
// #[cfg(target_arch = "wasm32")]
// pub mod firestore {}

// TODO(async-wasm): Re-enable functions module as part of Stage 3.
// #[cfg(not(target_arch = "wasm32"))]
// pub mod functions;
// #[cfg(target_arch = "wasm32")]
// pub mod functions {}

// TODO(async-wasm): Re-enable installations module during Stage 3.
// #[cfg(not(target_arch = "wasm32"))]
// pub mod installations;
// #[cfg(target_arch = "wasm32")]
// pub mod installations {}
pub mod logger;
// TODO(async-wasm): Re-enable messaging when Stage 2 migrates push components.
// #[cfg(not(target_arch = "wasm32"))]
// pub mod messaging;
// #[cfg(target_arch = "wasm32")]
// pub mod messaging {}

// TODO(async-wasm): Re-enable performance module in Stage 3.
// #[cfg(not(target_arch = "wasm32"))]
// pub mod performance;
// #[cfg(target_arch = "wasm32")]
// pub mod performance {}
#[cfg(not(target_arch = "wasm32"))]
pub mod platform;
#[cfg(target_arch = "wasm32")]
pub mod platform {}
// TODO(async-wasm): Re-enable remote_config once Stage 3 async transport is ready.
// #[cfg(not(target_arch = "wasm32"))]
// pub mod remote_config;
// #[cfg(target_arch = "wasm32")]
// pub mod remote_config {}

// TODO(async-wasm): Re-enable storage when Stage 3 auditing completes.
// #[cfg(not(target_arch = "wasm32"))]
// pub mod storage;
// #[cfg(target_arch = "wasm32")]
// pub mod storage {}
pub mod util;

#[cfg(test)]
pub mod test_support;
