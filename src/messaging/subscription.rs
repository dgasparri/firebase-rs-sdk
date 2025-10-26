//! Push subscription helpers for Firebase Messaging.
//!
//! Mirrors the logic in `packages/messaging/src/internals/token-manager.ts` regarding
//! interaction with the browser `PushManager`.

use crate::messaging::error::{unsupported_browser, MessagingResult};

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
mod wasm {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;

    use crate::messaging::error::{
        internal_error, invalid_argument, token_subscribe_failed, token_unsubscribe_failed,
        unsupported_browser, MessagingResult,
    };
    use crate::messaging::sw_manager::ServiceWorkerRegistrationHandle;

    /// Data extracted from an active `PushSubscription`.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct PushSubscriptionDetails {
        pub endpoint: String,
        pub auth: String,
        pub p256dh: String,
    }

    /// Wrapper around `web_sys::PushSubscription` that exposes helper methods.
    #[derive(Clone)]
    pub struct PushSubscriptionHandle {
        inner: web_sys::PushSubscription,
    }

    impl PushSubscriptionHandle {
        fn new(inner: web_sys::PushSubscription) -> Self {
            Self { inner }
        }

        pub fn as_web_sys(&self) -> &web_sys::PushSubscription {
            &self.inner
        }

        pub fn details(&self) -> MessagingResult<PushSubscriptionDetails> {
            let endpoint = self
                .inner
                .endpoint()
                .ok_or_else(|| token_subscribe_failed("Push subscription missing endpoint"))?;
            let auth = extract_key(&self.inner, "auth")?;
            let p256dh = extract_key(&self.inner, "p256dh")?;

            Ok(PushSubscriptionDetails {
                endpoint,
                auth,
                p256dh,
            })
        }

        pub async fn unsubscribe(self) -> MessagingResult<bool> {
            let promise = self
                .inner
                .unsubscribe()
                .map_err(|err| token_unsubscribe_failed(format_js_error("unsubscribe", err)))?;

            let result = JsFuture::from(promise)
                .await
                .map_err(|err| token_unsubscribe_failed(format_js_error("unsubscribe", err)))?;

            Ok(result.as_bool().unwrap_or(true))
        }
    }

    #[derive(Default)]
    pub struct PushSubscriptionManager {
        cached: Option<PushSubscriptionHandle>,
    }

    impl PushSubscriptionManager {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn cached_subscription(&self) -> Option<PushSubscriptionHandle> {
            self.cached.clone()
        }

        pub async fn subscribe(
            &mut self,
            registration: &ServiceWorkerRegistrationHandle,
            vapid_key: &str,
        ) -> MessagingResult<PushSubscriptionHandle> {
            if let Some(handle) = &self.cached {
                return Ok(handle.clone());
            }

            let sw_registration = registration.as_web_sys();
            let push_manager = sw_registration
                .push_manager()
                .map_err(|err| unsupported_browser(format_js_error("pushManager", err)))?;

            let existing = JsFuture::from(
                push_manager
                    .get_subscription()
                    .map_err(|err| internal_error(format_js_error("getSubscription", err)))?,
            )
            .await
            .map_err(|err| internal_error(format_js_error("getSubscription", err)))?;

            let subscription = if existing.is_undefined() || existing.is_null() {
                let mut options = web_sys::PushSubscriptionOptionsInit::new();
                options.user_visible_only(true);
                let application_server_key = vapid_key_to_uint8_array(vapid_key)?;
                options.application_server_key(Some(&application_server_key));

                let promise = push_manager
                    .subscribe_with_options(&options)
                    .map_err(|err| token_subscribe_failed(format_js_error("subscribe", err)))?;

                let value = JsFuture::from(promise)
                    .await
                    .map_err(|err| token_subscribe_failed(format_js_error("subscribe", err)))?;

                value.dyn_into().map_err(|_| {
                    token_subscribe_failed("PushManager.subscribe returned unexpected value")
                })?
            } else {
                existing.dyn_into().map_err(|_| {
                    token_subscribe_failed("getSubscription returned unexpected value")
                })?
            };

            let handle = PushSubscriptionHandle::new(subscription);
            self.cached = Some(handle.clone());
            Ok(handle)
        }

        pub fn clear_cache(&mut self) {
            self.cached = None;
        }
    }

    fn vapid_key_to_uint8_array(vapid_key: &str) -> MessagingResult<js_sys::Uint8Array> {
        let trimmed = vapid_key.trim();
        if trimmed.is_empty() {
            return Err(invalid_argument("VAPID key must not be empty"));
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(trimmed)
            .map_err(|err| invalid_argument(format!("Invalid VAPID key: {err}")))?;
        Ok(js_sys::Uint8Array::from(bytes.as_slice()))
    }

    fn extract_key(
        subscription: &web_sys::PushSubscription,
        name: &str,
    ) -> MessagingResult<String> {
        if let Some(buffer) = subscription.get_key(name) {
            let view = js_sys::Uint8Array::new(&buffer);
            Ok(base64::engine::general_purpose::STANDARD.encode(view.to_vec()))
        } else {
            Err(token_subscribe_failed(format!(
                "Push subscription missing {name} key"
            )))
        }
    }

    fn format_js_error(operation: &str, err: JsValue) -> String {
        let detail = err.as_string().unwrap_or_else(|| format!("{:?}", err));
        format!("{operation} failed: {detail}")
    }

    pub use PushSubscriptionDetails as Details;
    pub use PushSubscriptionHandle as Handle;
    pub use PushSubscriptionManager as Manager;
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
pub use wasm::{
    Details as PushSubscriptionDetails, Handle as PushSubscriptionHandle,
    Manager as PushSubscriptionManager,
};

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
#[derive(Default)]
pub struct PushSubscriptionManager;

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
impl PushSubscriptionManager {
    pub fn new() -> Self {
        Self
    }

    pub fn cached_subscription(&self) -> Option<PushSubscriptionHandle> {
        None
    }

    pub async fn subscribe(
        &mut self,
        _registration: &crate::messaging::ServiceWorkerRegistrationHandle,
        _vapid_key: &str,
    ) -> MessagingResult<PushSubscriptionHandle> {
        Err(unsupported_browser(
            "Push subscriptions are only available when the `wasm-web` feature is enabled.",
        ))
    }

    pub fn clear_cache(&mut self) {}
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
#[derive(Clone, Debug)]
pub struct PushSubscriptionHandle;

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushSubscriptionDetails;

#[cfg(all(test, not(all(feature = "wasm-web", target_arch = "wasm32"))))]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn native_subscribe_reports_unsupported() {
        let mut manager = PushSubscriptionManager::new();
        manager.clear_cache();
        assert!(manager.cached_subscription().is_none());
        let registration = crate::messaging::ServiceWorkerRegistrationHandle;
        let err = manager.subscribe(&registration, "test").await.unwrap_err();
        assert_eq!(err.code_str(), "messaging/unsupported-browser");
    }
}
