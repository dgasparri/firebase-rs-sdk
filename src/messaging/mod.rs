#![doc = include_str!("README.md")]
mod api;
mod constants;
mod error;
#[cfg(any(test, all(feature = "wasm-web", target_arch = "wasm32")))]
mod fcm_rest;
mod subscription;
mod support;
mod sw_manager;
mod token_store;
mod types;

pub use api::{
    get_messaging, on_background_message, on_message, register_messaging_component, Messaging, PermissionState,
};

pub use error::{
    available_in_service_worker, available_in_window, failed_default_registration, internal_error, invalid_argument,
    invalid_service_worker_registration, permission_blocked, token_deletion_failed, token_subscribe_failed,
    token_subscribe_no_token, token_unsubscribe_failed, token_update_failed, token_update_no_token,
    unsupported_browser, MessagingError, MessagingErrorCode, MessagingResult,
};

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
pub use subscription::PushSubscriptionManager;
#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    not(feature = "experimental-indexed-db")
))]
pub use subscription::PushSubscriptionManager;
#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
pub use subscription::{PushSubscriptionDetails, PushSubscriptionHandle, PushSubscriptionManager};
pub use support::is_supported;
#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
pub use sw_manager::{ServiceWorkerManager, ServiceWorkerRegistrationHandle};
#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    not(feature = "experimental-indexed-db")
))]
pub use sw_manager::{ServiceWorkerManager, ServiceWorkerRegistrationHandle};
#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
pub use sw_manager::{ServiceWorkerManager, ServiceWorkerRegistrationHandle};

pub use token_store::{read_token, remove_token, write_token, InstallationInfo, SubscriptionInfo, TokenRecord};

pub use types::{FcmOptions, MessageHandler, MessagePayload, NotificationPayload, Unsubscribe};
