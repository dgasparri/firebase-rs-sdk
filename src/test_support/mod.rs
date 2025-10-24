//! Test utilities shared across crate-level unit and integration tests.

pub mod firebase;
#[cfg(not(target_arch = "wasm32"))]
pub mod http;

pub use firebase::test_firebase_app_with_api_key;
#[cfg(not(target_arch = "wasm32"))]
pub use http::start_mock_server;
