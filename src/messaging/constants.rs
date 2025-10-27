pub const MESSAGING_COMPONENT_NAME: &str = "messaging";

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
pub const DEFAULT_SW_PATH: &str = "/firebase-messaging-sw.js";
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
pub const DEFAULT_SW_SCOPE: &str = "/firebase-cloud-messaging-push-scope";
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
pub const DEFAULT_REGISTRATION_TIMEOUT_MS: i32 = 10_000;
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
pub const REGISTRATION_POLL_INTERVAL_MS: i32 = 100;
#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
pub const DEFAULT_VAPID_KEY: &str =
    "BDOU99-h67HcA6JeFXHbSNMu7e2yNNu3RzoMj8TM4W88jITfq7ZmPvIM1Iv-4_l2LxQcYwhqby2xGpWwzjfAnG4";

#[allow(dead_code)]
pub const FCM_RETRY_BASE_DELAY_MS: u64 = 5_000;
#[allow(dead_code)]
pub const FCM_MAX_RETRIES: u32 = 3;
