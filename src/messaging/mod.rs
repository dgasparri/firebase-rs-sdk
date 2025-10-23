#![doc = include_str!("README.md")]
mod api;
mod constants;
pub mod error;
#[cfg(any(test, all(feature = "wasm-web", target_arch = "wasm32")))]
mod fcm_rest;
mod subscription;
mod support;
mod sw_manager;
pub(crate) mod token_store;
mod types;

pub use api::{
    get_messaging, on_background_message, on_message, register_messaging_component, Messaging,
    PermissionState,
};
#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
pub use subscription::PushSubscriptionManager;
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
pub use subscription::{PushSubscriptionDetails, PushSubscriptionHandle, PushSubscriptionManager};
pub use support::is_supported;
#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
pub use sw_manager::ServiceWorkerRegistrationHandle;
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
pub use sw_manager::{ServiceWorkerManager, ServiceWorkerRegistrationHandle};

pub use sw_manager::ServiceWorkerManager;
pub use types::{FcmOptions, MessageHandler, MessagePayload, NotificationPayload, Unsubscribe};
