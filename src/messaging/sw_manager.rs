#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
mod wasm {
    use std::cell::RefCell;

    use js_sys::{Date, Reflect};
    use serde::{Deserialize, Serialize};
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::{spawn_local, JsFuture};

    use crate::messaging::constants::{
        DEFAULT_REGISTRATION_TIMEOUT_MS, DEFAULT_SW_PATH, DEFAULT_SW_SCOPE, REGISTRATION_POLL_INTERVAL_MS,
    };
    use crate::messaging::error::{
        available_in_window, failed_default_registration, unsupported_browser, MessagingResult,
    };
    use crate::platform::runtime;
    use web_sys::{BroadcastChannel, MessageEvent, StorageEvent};

    const REGISTRATION_BROADCAST_CHANNEL: &str = "firebase-messaging-service-worker-updates";
    const REGISTRATION_STORAGE_KEY: &str = "firebase-messaging-sw-updates";

    thread_local! {
        static SHARED_REGISTRATION: RefCell<Option<ServiceWorkerRegistrationHandle>> = RefCell::new(None);
        static BROADCAST_CHANNEL: RefCell<Option<BroadcastChannel>> = RefCell::new(None);
        static BROADCAST_HANDLER: RefCell<Option<Closure<dyn FnMut(MessageEvent)>>> = RefCell::new(None);
        static STORAGE_HANDLER: RefCell<Option<Closure<dyn FnMut(StorageEvent)>>> = RefCell::new(None);
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct RegistrationBroadcast {
        scope: String,
        timestamp_ms: u64,
    }

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
            ensure_cross_context_sync();
            Self::default()
        }

        /// Returns the cached service worker registration, if one was previously stored.
        pub fn registration(&self) -> Option<ServiceWorkerRegistrationHandle> {
            self.registration.clone().or_else(shared_registration)
        }

        /// Caches a user-supplied `ServiceWorkerRegistration`.
        pub fn use_registration(
            &mut self,
            registration: web_sys::ServiceWorkerRegistration,
        ) -> ServiceWorkerRegistrationHandle {
            let handle = ServiceWorkerRegistrationHandle::new(registration);
            self.cache_registration(handle.clone());
            broadcast_registration_update(&handle.as_web_sys().scope());
            handle
        }

        /// Registers the default Firebase Messaging service worker and waits until it activates.
        pub async fn register_default(&mut self) -> MessagingResult<ServiceWorkerRegistrationHandle> {
            ensure_cross_context_sync();

            if let Some(handle) = self.registration() {
                return Ok(handle.clone());
            }

            let container = service_worker_container()?;

            if let Ok(Some(existing)) = find_existing_registration(&container).await {
                wait_for_registration_active(&existing).await?;
                let handle = ServiceWorkerRegistrationHandle::new(existing);
                self.cache_registration(handle.clone());
                return Ok(handle);
            }

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
            self.cache_registration(handle.clone());
            broadcast_registration_update(DEFAULT_SW_SCOPE);
            Ok(handle)
        }

        fn cache_registration(&mut self, handle: ServiceWorkerRegistrationHandle) {
            self.registration = Some(handle.clone());
            SHARED_REGISTRATION.with(|slot| {
                slot.borrow_mut().replace(handle);
            });
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

    async fn find_existing_registration(
        container: &web_sys::ServiceWorkerContainer,
    ) -> MessagingResult<Option<web_sys::ServiceWorkerRegistration>> {
        find_registration_for_scope(container, DEFAULT_SW_SCOPE).await
    }

    fn service_worker_container() -> MessagingResult<web_sys::ServiceWorkerContainer> {
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
        container_value
            .dyn_into()
            .map_err(|_| unsupported_browser("Service workers are not available in this browser environment."))
    }

    fn format_js_error(operation: &str, err: JsValue) -> String {
        let detail = err.as_string().unwrap_or_else(|| format!("{:?}", err));
        format!("{operation} failed: {detail}")
    }

    fn shared_registration() -> Option<ServiceWorkerRegistrationHandle> {
        SHARED_REGISTRATION.with(|slot| slot.borrow().clone())
    }

    fn ensure_cross_context_sync() {
        init_broadcast_channel();
        init_storage_listener();
    }

    fn init_broadcast_channel() {
        BROADCAST_CHANNEL.with(|cell| {
            if cell.borrow().is_some() {
                return;
            }

            match BroadcastChannel::new(REGISTRATION_BROADCAST_CHANNEL) {
                Ok(channel) => {
                    let handler = Closure::wrap(Box::new(move |event: MessageEvent| {
                        if let Some(text) = event.data().as_string() {
                            if let Ok(message) = serde_json::from_str::<RegistrationBroadcast>(&text) {
                                handle_registration_message(message);
                            }
                        }
                    }) as Box<dyn FnMut(_)>);
                    channel.set_onmessage(Some(handler.as_ref().unchecked_ref()));
                    BROADCAST_HANDLER.with(|slot| {
                        slot.replace(Some(handler));
                    });
                    cell.replace(Some(channel));
                }
                Err(err) => log_warning("Failed to initialize ServiceWorker BroadcastChannel", Some(&err)),
            }
        });
    }

    fn init_storage_listener() {
        STORAGE_HANDLER.with(|slot| {
            if slot.borrow().is_some() {
                return;
            }

            if let Some(window) = web_sys::window() {
                let handler = Closure::wrap(Box::new(move |event: StorageEvent| {
                    if let Some(key) = event.key() {
                        if key != REGISTRATION_STORAGE_KEY {
                            return;
                        }
                    } else {
                        return;
                    }

                    if let Some(value) = event.new_value().or_else(|| event.old_value()) {
                        if let Ok(message) = serde_json::from_str::<RegistrationBroadcast>(&value) {
                            handle_registration_message(message);
                        }
                    }
                }) as Box<dyn FnMut(_)>);

                if let Err(err) = window.add_event_listener_with_callback("storage", handler.as_ref().unchecked_ref()) {
                    log_warning(
                        "Failed to attach storage listener for messaging ServiceWorker updates",
                        Some(&err),
                    );
                } else {
                    slot.replace(Some(handler));
                }
            }
        });
    }

    fn handle_registration_message(message: RegistrationBroadcast) {
        let scope = message.scope.clone();
        spawn_local(async move {
            if let Err(err) = sync_registration_from_scope(&scope).await {
                log_warning_text(
                    "Failed to synchronise messaging service worker registration",
                    &format!("{err:?}"),
                );
            }
        });
    }

    async fn sync_registration_from_scope(scope: &str) -> MessagingResult<()> {
        let container = service_worker_container()?;
        if let Some(registration) = find_registration_for_scope(&container, scope).await? {
            wait_for_registration_active(&registration).await?;
            SHARED_REGISTRATION.with(|slot| {
                slot.borrow_mut()
                    .replace(ServiceWorkerRegistrationHandle::new(registration));
            });
        }

        Ok(())
    }

    async fn find_registration_for_scope(
        container: &web_sys::ServiceWorkerContainer,
        scope: &str,
    ) -> MessagingResult<Option<web_sys::ServiceWorkerRegistration>> {
        let promise = container.get_registration_with_document_url(scope);
        let registration_js = JsFuture::from(promise)
            .await
            .map_err(|err| failed_default_registration(format_js_error("serviceWorker.getRegistration", err)))?;
        if registration_js.is_null() || registration_js.is_undefined() {
            return Ok(None);
        }

        registration_js
            .dyn_into()
            .map(Some)
            .map_err(|_| failed_default_registration("Unexpected value from serviceWorker.getRegistration"))
    }

    fn broadcast_registration_update(scope: &str) {
        let message = RegistrationBroadcast {
            scope: scope.to_string(),
            timestamp_ms: Date::now() as u64,
        };
        let serialized = match serde_json::to_string(&message) {
            Ok(value) => value,
            Err(_) => return,
        };

        BROADCAST_CHANNEL.with(|cell| {
            if cell.borrow().is_none() {
                init_broadcast_channel();
            }
        });

        BROADCAST_CHANNEL.with(|cell| {
            if let Some(channel) = cell.borrow().as_ref() {
                if let Err(err) = channel.post_message(&JsValue::from_str(&serialized)) {
                    log_warning("Failed to broadcast service worker update", Some(&err));
                }
            }
        });

        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                if let Err(err) = storage.set_item(REGISTRATION_STORAGE_KEY, &serialized) {
                    log_warning("Failed to publish service worker update via storage", Some(&err));
                }
            }
        }
    }

    fn log_warning(message: &str, err: Option<&JsValue>) {
        if let Some(err) = err {
            web_sys::console::warn_2(&JsValue::from_str(message), err);
        } else {
            web_sys::console::warn_1(&JsValue::from_str(message));
        }
    }

    fn log_warning_text(message: &str, detail: &str) {
        web_sys::console::warn_1(&JsValue::from_str(&format!("{message}: {detail}")));
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
