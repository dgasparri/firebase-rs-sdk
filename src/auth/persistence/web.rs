use std::sync::Arc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use web_sys::{self, BroadcastChannel, EventTarget, MessageEvent, Storage, StorageEvent, Window};

use crate::auth::error::{AuthError, AuthResult};

use super::{AuthPersistence, PersistedAuthState, PersistenceListener, PersistenceSubscription};

const DEFAULT_STORAGE_KEY: &str = "firebase:authUser";
const DEFAULT_CHANNEL_NAME: &str = "firebase-auth-uplink";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebStorageDriver {
    Local,
    Session,
}

#[derive(Debug, Clone)]
pub struct WebStoragePersistence {
    key: Arc<String>,
    channel_name: Arc<String>,
    driver: WebStorageDriver,
}

impl WebStoragePersistence {
    pub fn new(driver: WebStorageDriver) -> Self {
        Self::with_key_and_channel(driver, DEFAULT_STORAGE_KEY, DEFAULT_CHANNEL_NAME)
    }

    pub fn with_key(driver: WebStorageDriver, key: impl Into<String>) -> Self {
        Self::with_key_and_channel(driver, key, DEFAULT_CHANNEL_NAME)
    }

    pub fn with_key_and_channel(
        driver: WebStorageDriver,
        key: impl Into<String>,
        channel_name: impl Into<String>,
    ) -> Self {
        Self {
            key: Arc::new(key.into()),
            channel_name: Arc::new(channel_name.into()),
            driver,
        }
    }

    fn storage(&self, window: &Window) -> Result<Storage, AuthError> {
        match self.driver {
            WebStorageDriver::Local => window.local_storage().map_err(map_js_error)?,
            WebStorageDriver::Session => window.session_storage().map_err(map_js_error)?,
        }
        .ok_or_else(|| AuthError::InvalidCredential("Web storage API is unavailable".into()))
    }

    fn window() -> Result<Window, AuthError> {
        web_sys::window()
            .ok_or_else(|| AuthError::InvalidCredential("window object is not available in this environment".into()))
    }

    fn deserialize(value: &str) -> Option<PersistedAuthState> {
        if value.is_empty() {
            return None;
        }

        serde_json::from_str(value).ok()
    }

    fn serialize(state: &PersistedAuthState) -> Result<String, AuthError> {
        serde_json::to_string(state)
            .map_err(|err| AuthError::InvalidCredential(format!("Failed to serialize auth persistence payload: {err}")))
    }

    fn notify_via_broadcast(&self, payload: Option<&str>) {
        if let Ok(channel) = BroadcastChannel::new(self.channel_name.as_ref()) {
            let message = match payload {
                Some(value) => JsValue::from_str(value),
                None => JsValue::NULL,
            };
            if let Err(err) = channel.post_message(&message) {
                log_js_error("postMessage", err);
            }
        }
    }

    fn parse_broadcast_message(event: &MessageEvent) -> Option<Option<PersistedAuthState>> {
        let data = event.data();
        if data.is_null() || data.is_undefined() {
            return Some(None);
        }

        if let Some(text) = data.as_string() {
            return Some(Self::deserialize(&text));
        }

        // Attempt to stringify non-string payloads.
        match js_sys::JSON::stringify(&data) {
            Ok(value) => value.as_string().map(|text| Self::deserialize(&text)),
            Err(_) => None,
        }
    }
}

impl AuthPersistence for WebStoragePersistence {
    fn set(&self, state: Option<PersistedAuthState>) -> AuthResult<()> {
        let window = Self::window()?;
        let storage = self.storage(&window)?;

        match state {
            Some(ref state) => {
                let serialized = Self::serialize(state)?;
                storage.set_item(self.key.as_ref(), &serialized).map_err(map_js_error)?;
                self.notify_via_broadcast(Some(&serialized));
            }
            None => {
                storage.remove_item(self.key.as_ref()).map_err(map_js_error)?;
                self.notify_via_broadcast(None);
            }
        }

        Ok(())
    }

    fn get(&self) -> AuthResult<Option<PersistedAuthState>> {
        let window = Self::window()?;
        let storage = self.storage(&window)?;
        let value = storage.get_item(self.key.as_ref()).map_err(map_js_error)?;

        Ok(value.and_then(|string| Self::deserialize(&string)))
    }

    fn subscribe(&self, listener: PersistenceListener) -> AuthResult<PersistenceSubscription> {
        let window = Self::window()?;
        let key = self.key.clone();
        let storage_listener = listener.clone();
        let storage_closure = Closure::wrap(Box::new(move |event: StorageEvent| {
            if let Some(event_key) = event.key() {
                if event_key != *key {
                    return;
                }
            } else {
                storage_listener(None);
                return;
            }

            let new_state = event
                .new_value()
                .and_then(|value| WebStoragePersistence::deserialize(&value));
            storage_listener(new_state);
        }) as Box<dyn FnMut(StorageEvent)>);

        let storage_handle = StorageListenerHandle::attach(window.clone().into(), "storage", storage_closure)?;

        let listener_clone = listener.clone();
        let broadcast_handle = match BroadcastChannel::new(self.channel_name.as_ref()) {
            Ok(channel) => {
                let broadcast_closure = Closure::wrap(Box::new(move |event: MessageEvent| {
                    if let Some(parsed) = WebStoragePersistence::parse_broadcast_message(&event) {
                        listener_clone(parsed);
                    }
                }) as Box<dyn FnMut(MessageEvent)>);
                Some(BroadcastListenerHandle::attach(channel, broadcast_closure))
            }
            Err(err) => {
                log_js_error("BroadcastChannel::new", err);
                None
            }
        };

        let cleanup = move || {
            drop(storage_handle);
            if let Some(handle) = broadcast_handle {
                drop(handle);
            }
        };

        Ok(PersistenceSubscription::new(cleanup))
    }
}

fn map_js_error(err: JsValue) -> AuthError {
    AuthError::InvalidCredential(format!("Web storage error: {}", stringify_js_error(err)))
}

fn stringify_js_error(err: JsValue) -> String {
    if let Some(string) = err.as_string() {
        return string;
    }

    if let Ok(stringified) = js_sys::JSON::stringify(&err) {
        if let Some(text) = stringified.as_string() {
            return text;
        }
    }

    format!("{err:?}")
}

fn log_js_error(context: &str, err: JsValue) {
    web_sys::console::error_2(&JsValue::from_str(context), &err);
}

struct StorageListenerHandle {
    target: EventTarget,
    event: &'static str,
    callback: Closure<dyn FnMut(StorageEvent)>,
}

impl StorageListenerHandle {
    fn attach(
        target: EventTarget,
        event: &'static str,
        callback: Closure<dyn FnMut(StorageEvent)>,
    ) -> Result<Self, AuthError> {
        target
            .add_event_listener_with_callback(event, callback.as_ref().unchecked_ref())
            .map_err(map_js_error)?;
        Ok(Self {
            target,
            event,
            callback,
        })
    }
}

impl Drop for StorageListenerHandle {
    fn drop(&mut self) {
        if let Err(err) = self
            .target
            .remove_event_listener_with_callback(self.event, self.callback.as_ref().unchecked_ref())
        {
            log_js_error("removeEventListener", err);
        }
    }
}

unsafe impl Send for StorageListenerHandle {}
unsafe impl Sync for StorageListenerHandle {}

struct BroadcastListenerHandle {
    channel: BroadcastChannel,
    #[allow(dead_code)]
    callback: Closure<dyn FnMut(MessageEvent)>,
}

impl BroadcastListenerHandle {
    fn attach(channel: BroadcastChannel, callback: Closure<dyn FnMut(MessageEvent)>) -> Self {
        channel.set_onmessage(Some(callback.as_ref().unchecked_ref()));
        Self { channel, callback }
    }
}

impl Drop for BroadcastListenerHandle {
    fn drop(&mut self) {
        self.channel.set_onmessage(None);
    }
}

unsafe impl Send for BroadcastListenerHandle {}
unsafe impl Sync for BroadcastListenerHandle {}
