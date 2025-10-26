use crate::messaging::error::{unsupported_browser, MessagingResult};

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
mod wasm {
    use std::rc::Rc;

    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;

    use crate::messaging::constants::{
        DEFAULT_REGISTRATION_TIMEOUT_MS, DEFAULT_SW_PATH, DEFAULT_SW_SCOPE,
        REGISTRATION_POLL_INTERVAL_MS,
    };
    use crate::messaging::error::{
        available_in_window, failed_default_registration, internal_error, unsupported_browser,
        MessagingResult,
    };

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
        pub async fn register_default(
            &mut self,
        ) -> MessagingResult<ServiceWorkerRegistrationHandle> {
            if let Some(handle) = &self.registration {
                return Ok(handle.clone());
            }

            let window = web_sys::window().ok_or_else(|| {
                available_in_window("Service worker registration requires a Window context")
            })?;
            let navigator = window.navigator();
            let container = navigator.service_worker().ok_or_else(|| {
                unsupported_browser(
                    "Service workers are not available in this browser environment.",
                )
            })?;

            let mut options = web_sys::RegistrationOptions::new();
            options.scope(DEFAULT_SW_SCOPE);

            let promise = container
                .register_with_str_and_options(DEFAULT_SW_PATH, &options)
                .map_err(|err| internal_error(format_js_error("serviceWorker.register", err)))?;
            let registration_js = JsFuture::from(promise).await.map_err(|err| {
                failed_default_registration(format_js_error("serviceWorker.register", err))
            })?;
            let registration: web_sys::ServiceWorkerRegistration =
                registration_js.dyn_into().map_err(|_| {
                    failed_default_registration(
                        "Unexpected return value from serviceWorker.register",
                    )
                })?;

            if let Ok(update_promise) = registration.update() {
                let _ = JsFuture::from(update_promise).await;
            }

            wait_for_registration_active(&registration).await?;

            let handle = ServiceWorkerRegistrationHandle::new(registration);
            self.registration = Some(handle.clone());
            Ok(handle)
        }
    }

    async fn wait_for_registration_active(
        registration: &web_sys::ServiceWorkerRegistration,
    ) -> MessagingResult<()> {
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
        let window = web_sys::window().ok_or_else(|| {
            available_in_window("Timers require a Window context for service worker polling")
        })?;
        let window = Rc::new(window);

        let promise = js_sys::Promise::new(&mut |resolve, reject| {
            let resolve_fn = resolve.unchecked_into::<js_sys::Function>();
            let reject_fn = reject.unchecked_into::<js_sys::Function>();
            let window = Rc::clone(&window);

            let closure = Closure::once(move || {
                let _ = resolve_fn.call0(&JsValue::UNDEFINED);
            });

            if window
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    closure.as_ref().unchecked_ref(),
                    ms,
                )
                .is_ok()
            {
                closure.forget();
            } else {
                // If setTimeout fails we propagate an error through the promise rejection path.
                let error = js_sys::Error::new("Failed to schedule timeout");
                let _ = reject_fn.call1(&JsValue::UNDEFINED, &error);
            }
        });

        JsFuture::from(promise)
            .await
            .map(|_| ())
            .map_err(|err| internal_error(format_js_error("setTimeout", err)))
    }

    fn format_js_error(operation: &str, err: JsValue) -> String {
        let detail = err.as_string().unwrap_or_else(|| format!("{:?}", err));
        format!("{operation} failed: {detail}")
    }

    pub use ServiceWorkerManager as Manager;
    pub use ServiceWorkerRegistrationHandle as Handle;
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
pub use wasm::{Handle as ServiceWorkerRegistrationHandle, Manager as ServiceWorkerManager};

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
#[derive(Default)]
pub struct ServiceWorkerManager;

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
impl ServiceWorkerManager {
    pub fn new() -> Self {
        Self
    }

    pub fn registration(&self) -> Option<ServiceWorkerRegistrationHandle> {
        None
    }

    pub async fn register_default(&mut self) -> MessagingResult<ServiceWorkerRegistrationHandle> {
        Err(unsupported_browser(
            "Service worker registration is only available when the `wasm-web` feature is enabled.",
        ))
    }
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
#[derive(Clone, Debug)]
pub struct ServiceWorkerRegistrationHandle;

#[cfg(all(test, not(all(feature = "wasm-web", target_arch = "wasm32"))))]
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
