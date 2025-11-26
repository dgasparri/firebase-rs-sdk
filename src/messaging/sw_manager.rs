#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
mod wasm {
    use js_sys::Reflect;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;

    use crate::messaging::constants::{
        DEFAULT_REGISTRATION_TIMEOUT_MS, DEFAULT_SW_PATH, DEFAULT_SW_SCOPE, REGISTRATION_POLL_INTERVAL_MS,
    };
    use crate::messaging::error::{
        available_in_window, failed_default_registration, unsupported_browser, MessagingResult,
    };
    use crate::platform::runtime;

    /// Thin wrapper around a `ServiceWorkerRegistration` reference.
    #[derive(Clone)]
    pub struct ServiceWorkerRegistrationHandle {
        inner: web_sys::ServiceWorkerRegistration,
    }

    impl ServiceWorkerRegistrationHandle {
        fn new(inner: web_sys::ServiceWorkerRegistration) -> Self {
            Self { inner }
        }

        /// Returns the underlying `ServiceWorkerRegistration` handle.
        pub fn as_web_sys(&self) -> &web_sys::ServiceWorkerRegistration {
            &self.inner
        }
    }

    /// Coordinates service worker registration for messaging.
    ///
    /// Port of the logic in `packages/messaging/src/helpers/registerDefaultSw.ts`.
    #[derive(Default)]
    pub struct ServiceWorkerManager {
        registration: Option<ServiceWorkerRegistrationHandle>,
    }

    impl ServiceWorkerManager {
        pub fn new() -> Self {
            Self::default()
        }

        /// Returns the cached service worker registration, if one was previously stored.
        pub fn registration(&self) -> Option<ServiceWorkerRegistrationHandle> {
            self.registration.clone()
        }

        /// Caches a user-supplied `ServiceWorkerRegistration`.
        pub fn use_registration(
            &mut self,
            registration: web_sys::ServiceWorkerRegistration,
        ) -> ServiceWorkerRegistrationHandle {
            let handle = ServiceWorkerRegistrationHandle::new(registration);
            self.registration = Some(handle.clone());
            handle
        }

        /// Registers the default Firebase Messaging service worker and waits until it activates.
        pub async fn register_default(&mut self) -> MessagingResult<ServiceWorkerRegistrationHandle> {
            if let Some(handle) = &self.registration {
                return Ok(handle.clone());
            }

            let window = web_sys::window()
                .ok_or_else(|| available_in_window("Service worker registration requires a Window context"))?;
            let navigator = window.navigator();
            let navigator_js = JsValue::from(navigator.clone());
            let container_value = Reflect::get(&navigator_js, &JsValue::from_str("serviceWorker"))
                .map_err(|_| unsupported_browser("Service workers are not available in this browser environment."))?;
            if container_value.is_undefined() || container_value.is_null() {
                return Err(unsupported_browser(
                    "Service workers are not available in this browser environment.",
                ));
            }
            let container: web_sys::ServiceWorkerContainer = container_value
                .dyn_into()
                .map_err(|_| unsupported_browser("Service workers are not available in this browser environment."))?;

            let options = web_sys::RegistrationOptions::new();
            options.set_scope(DEFAULT_SW_SCOPE);

            let promise = container.register_with_options(DEFAULT_SW_PATH, &options);
            let registration_js = JsFuture::from(promise)
                .await
                .map_err(|err| failed_default_registration(format_js_error("serviceWorker.register", err)))?;
            let registration: web_sys::ServiceWorkerRegistration = registration_js
                .dyn_into()
                .map_err(|_| failed_default_registration("Unexpected return value from serviceWorker.register"))?;

            if let Ok(update_promise) = registration.update() {
                let _ = JsFuture::from(update_promise).await;
            }

            wait_for_registration_active(&registration).await?;

            let handle = ServiceWorkerRegistrationHandle::new(registration);
            self.registration = Some(handle.clone());
            Ok(handle)
        }
    }

    async fn wait_for_registration_active(registration: &web_sys::ServiceWorkerRegistration) -> MessagingResult<()> {
        if registration.active().is_some() {
            return Ok(());
        }

        let mut elapsed = 0;
        while elapsed < DEFAULT_REGISTRATION_TIMEOUT_MS {
            if registration.active().is_some() {
                return Ok(());
            }

            if registration.installing().is_none() && registration.waiting().is_none() {
                return Err(failed_default_registration(
                    "No incoming service worker found during registration.",
                ));
            }

            sleep_ms(REGISTRATION_POLL_INTERVAL_MS).await?;
            elapsed += REGISTRATION_POLL_INTERVAL_MS;
        }

        Err(failed_default_registration(format!(
            "Service worker not registered after {} ms",
            DEFAULT_REGISTRATION_TIMEOUT_MS
        )))
    }

    async fn sleep_ms(ms: i32) -> MessagingResult<()> {
        if ms <= 0 {
            return Ok(());
        }
        let duration = std::time::Duration::from_millis(ms as u64);
        runtime::sleep(duration).await;
        Ok(())
    }

    fn format_js_error(operation: &str, err: JsValue) -> String {
        let detail = err.as_string().unwrap_or_else(|| format!("{:?}", err));
        format!("{operation} failed: {detail}")
    }

    pub use ServiceWorkerManager as Manager;
    pub use ServiceWorkerRegistrationHandle as Handle;
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
pub use wasm::{Handle as ServiceWorkerRegistrationHandle, Manager as ServiceWorkerManager};

#[cfg(any(
    not(all(feature = "wasm-web", target_arch = "wasm32")),
    all(
        feature = "wasm-web",
        target_arch = "wasm32",
        not(feature = "experimental-indexed-db")
    )
))]
#[derive(Default)]
pub struct ServiceWorkerManager;

#[cfg(any(
    not(all(feature = "wasm-web", target_arch = "wasm32")),
    all(
        feature = "wasm-web",
        target_arch = "wasm32",
        not(feature = "experimental-indexed-db")
    )
))]
impl ServiceWorkerManager {
    pub fn new() -> Self {
        Self
    }

    pub fn registration(&self) -> Option<ServiceWorkerRegistrationHandle> {
        None
    }

    pub async fn register_default(
        &mut self,
    ) -> crate::messaging::error::MessagingResult<ServiceWorkerRegistrationHandle> {
        Err(crate::messaging::error::unsupported_browser(
            "Service worker registration is only available when the `wasm-web` feature is enabled.",
        ))
    }
}

#[cfg(any(
    not(all(feature = "wasm-web", target_arch = "wasm32")),
    all(
        feature = "wasm-web",
        target_arch = "wasm32",
        not(feature = "experimental-indexed-db")
    )
))]
#[derive(Clone, Debug)]
pub struct ServiceWorkerRegistrationHandle;

#[cfg(all(
    test,
    any(
        not(all(feature = "wasm-web", target_arch = "wasm32")),
        all(
            feature = "wasm-web",
            target_arch = "wasm32",
            not(feature = "experimental-indexed-db")
        )
    )
))]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn native_manager_reports_unsupported() {
        let mut manager = ServiceWorkerManager::new();
        assert!(manager.registration().is_none());
        let err = manager.register_default().await.unwrap_err();
        assert_eq!(err.code_str(), "messaging/unsupported-browser");
    }
}
