use serde::{Deserialize, Serialize};

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
use crate::messaging::error::internal_error;
use crate::messaging::error::MessagingResult;
#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
use crate::platform::browser::indexed_db;

#[cfg_attr(
    all(
        feature = "wasm-web",
        target_arch = "wasm32",
        not(feature = "experimental-indexed-db")
    ),
    allow(dead_code)
)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscriptionInfo {
    pub vapid_key: String,
    pub scope: String,
    pub endpoint: String,
    pub auth: String,
    pub p256dh: String,
}

#[cfg_attr(not(all(feature = "wasm-web", target_arch = "wasm32")), allow(dead_code))]
#[cfg_attr(
    all(
        feature = "wasm-web",
        target_arch = "wasm32",
        not(feature = "experimental-indexed-db")
    ),
    allow(dead_code)
)]
const AUTH_TOKEN_REFRESH_BUFFER_MS: u64 = 60_000;

#[cfg_attr(
    all(
        feature = "wasm-web",
        target_arch = "wasm32",
        not(feature = "experimental-indexed-db")
    ),
    allow(dead_code)
)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenRecord {
    pub token: String,
    pub create_time_ms: u64,
    pub subscription: Option<SubscriptionInfo>,
    pub installation: InstallationInfo,
}

#[cfg_attr(
    all(
        feature = "wasm-web",
        target_arch = "wasm32",
        not(feature = "experimental-indexed-db")
    ),
    allow(dead_code)
)]
impl TokenRecord {
    #[cfg_attr(not(all(feature = "wasm-web", target_arch = "wasm32")), allow(dead_code))]
    pub fn is_expired(&self, now_ms: u64, ttl_ms: u64) -> bool {
        now_ms.saturating_sub(self.create_time_ms) >= ttl_ms
    }
}

#[cfg_attr(
    all(
        feature = "wasm-web",
        target_arch = "wasm32",
        not(feature = "experimental-indexed-db")
    ),
    allow(dead_code)
)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallationInfo {
    pub fid: String,
    pub refresh_token: String,
    pub auth_token: String,
    pub auth_token_expiration_ms: u64,
}

#[cfg_attr(
    all(
        feature = "wasm-web",
        target_arch = "wasm32",
        not(feature = "experimental-indexed-db")
    ),
    allow(dead_code)
)]
impl InstallationInfo {
    #[cfg_attr(not(all(feature = "wasm-web", target_arch = "wasm32")), allow(dead_code))]
    pub fn auth_token_expired(&self, now_ms: u64) -> bool {
        now_ms + AUTH_TOKEN_REFRESH_BUFFER_MS >= self.auth_token_expiration_ms
    }
}

#[cfg_attr(
    all(
        feature = "wasm-web",
        target_arch = "wasm32",
        not(feature = "experimental-indexed-db")
    ),
    allow(dead_code)
)]
#[cfg(any(
    not(all(feature = "wasm-web", target_arch = "wasm32")),
    all(
        feature = "wasm-web",
        target_arch = "wasm32",
        not(feature = "experimental-indexed-db")
    )
))]
mod memory_store {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use once_cell::sync::Lazy;

    use super::{MessagingResult, TokenRecord};

    static STORE: Lazy<Mutex<HashMap<String, TokenRecord>>> = Lazy::new(|| Mutex::new(HashMap::new()));

    pub fn read(app_key: &str) -> MessagingResult<Option<TokenRecord>> {
        Ok(STORE.lock().unwrap().get(app_key).cloned())
    }

    pub fn write(app_key: &str, record: &TokenRecord) -> MessagingResult<()> {
        STORE.lock().unwrap().insert(app_key.to_string(), record.clone());
        Ok(())
    }

    pub fn remove(app_key: &str) -> MessagingResult<bool> {
        Ok(STORE.lock().unwrap().remove(app_key).is_some())
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
use std::cell::RefCell;
#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
use std::collections::HashMap;

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
use wasm_bindgen::closure::Closure;
#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
use wasm_bindgen::{JsCast, JsValue};
#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
use web_sys::{BroadcastChannel, MessageEvent};

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
const DATABASE_NAME: &str = "firebase-messaging-database";
#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
const DATABASE_VERSION: u32 = 1;
#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
const STORE_NAME: &str = "firebase-messaging-store";
#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
const BROADCAST_CHANNEL_NAME: &str = "firebase-messaging-token-updates";
#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
const REFRESH_LOCK_PREFIX: &str = "firebase-messaging-refresh-lock-";
#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
pub const REFRESH_LOCK_TTL_MS: u64 = 30_000;
#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
pub const REFRESH_WAIT_TIMEOUT_MS: u64 = 5_000;

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
#[derive(Serialize, Deserialize, Clone, Debug)]
struct BroadcastMessage {
    app_key: String,
    payload: BroadcastPayload,
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
#[derive(Serialize, Deserialize, Clone, Debug)]
enum BroadcastPayload {
    Set(TokenRecord),
    Remove,
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
#[derive(Serialize, Deserialize, Clone, Debug)]
struct RefreshLockState {
    owner: String,
    timestamp_ms: u64,
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
#[derive(Debug)]
struct TokenWaiter {
    app_key: String,
    sender: futures::channel::oneshot::Sender<()>,
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
thread_local! {
    static TOKEN_CACHE: RefCell<HashMap<String, Option<TokenRecord>>> = RefCell::new(HashMap::new());
    static BROADCAST_CHANNEL: RefCell<Option<BroadcastChannel>> = RefCell::new(None);
    static BROADCAST_HANDLER: RefCell<Option<Closure<dyn FnMut(MessageEvent)>>> = RefCell::new(None);
    static TOKEN_WAITERS: RefCell<Vec<TokenWaiter>> = RefCell::new(Vec::new());
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
pub fn read_token(app_key: &str) -> MessagingResult<Option<TokenRecord>> {
    memory_store::read(app_key)
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
pub async fn read_token(app_key: &str) -> MessagingResult<Option<TokenRecord>> {
    ensure_broadcast_channel();
    if let Some(cached) = cache_get(app_key) {
        return Ok(cached);
    }

    let db = open_db().await?;
    let stored = indexed_db::get_string(&db, STORE_NAME, app_key)
        .await
        .map_err(|err| internal_error(err.to_string()))?;
    let record = if let Some(json) = stored {
        let record: TokenRecord = serde_json::from_str(&json)
            .map_err(|err| internal_error(format!("Failed to parse stored token: {err}")))?;
        Some(record)
    } else {
        None
    };
    cache_set(app_key, record.clone());
    Ok(record)
}

#[cfg_attr(
    all(
        feature = "wasm-web",
        target_arch = "wasm32",
        not(feature = "experimental-indexed-db")
    ),
    allow(dead_code)
)]
#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    not(feature = "experimental-indexed-db")
))]
pub async fn read_token(app_key: &str) -> MessagingResult<Option<TokenRecord>> {
    Ok(memory_store::read(app_key)?)
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
pub fn write_token(app_key: &str, record: &TokenRecord) -> MessagingResult<()> {
    memory_store::write(app_key, record)
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
pub async fn write_token(app_key: &str, record: &TokenRecord) -> MessagingResult<()> {
    ensure_broadcast_channel();
    let json =
        serde_json::to_string(record).map_err(|err| internal_error(format!("Failed to serialize token: {err}")))?;
    let db = open_db().await?;
    indexed_db::put_string(&db, STORE_NAME, app_key, &json)
        .await
        .map_err(|err| internal_error(err.to_string()))?;
    cache_set(app_key, Some(record.clone()));
    notify_waiters(app_key);
    broadcast_update(app_key, BroadcastPayload::Set(record.clone()));
    Ok(())
}

#[cfg_attr(
    all(
        feature = "wasm-web",
        target_arch = "wasm32",
        not(feature = "experimental-indexed-db")
    ),
    allow(dead_code)
)]
#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    not(feature = "experimental-indexed-db")
))]
pub async fn write_token(app_key: &str, record: &TokenRecord) -> MessagingResult<()> {
    memory_store::write(app_key, record)?;
    Ok(())
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
pub fn remove_token(app_key: &str) -> MessagingResult<bool> {
    memory_store::remove(app_key)
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
pub async fn remove_token(app_key: &str) -> MessagingResult<bool> {
    ensure_broadcast_channel();
    let db = open_db().await?;
    let existed = indexed_db::get_string(&db, STORE_NAME, app_key)
        .await
        .map_err(|err| internal_error(err.to_string()))?
        .is_some();
    if existed {
        indexed_db::delete_key(&db, STORE_NAME, app_key)
            .await
            .map_err(|err| internal_error(err.to_string()))?;
        cache_set(app_key, None);
        notify_waiters(app_key);
        broadcast_update(app_key, BroadcastPayload::Remove);
    }
    Ok(existed)
}

#[cfg_attr(
    all(
        feature = "wasm-web",
        target_arch = "wasm32",
        not(feature = "experimental-indexed-db")
    ),
    allow(dead_code)
)]
#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    not(feature = "experimental-indexed-db")
))]
pub async fn remove_token(app_key: &str) -> MessagingResult<bool> {
    memory_store::remove(app_key)
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
async fn open_db() -> MessagingResult<web_sys::IdbDatabase> {
    indexed_db::open_database_with_store(DATABASE_NAME, DATABASE_VERSION, STORE_NAME)
        .await
        .map_err(|err| internal_error(err.to_string()))
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
fn cache_get(app_key: &str) -> Option<Option<TokenRecord>> {
    TOKEN_CACHE.with(|cache| cache.borrow().get(app_key).cloned())
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
fn cache_set(app_key: &str, value: Option<TokenRecord>) {
    TOKEN_CACHE.with(|cache| {
        cache.borrow_mut().insert(app_key.to_string(), value);
    });
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
fn ensure_broadcast_channel() {
    BROADCAST_CHANNEL.with(|channel_cell| {
        if channel_cell.borrow().is_some() {
            return;
        }

        match BroadcastChannel::new(BROADCAST_CHANNEL_NAME) {
            Ok(channel) => {
                let handler = Closure::wrap(Box::new(|event: MessageEvent| {
                    if let Some(text) = event.data().as_string() {
                        if let Ok(message) = serde_json::from_str::<BroadcastMessage>(&text) {
                            handle_broadcast_message(message);
                        }
                    }
                }) as Box<dyn FnMut(_)>);
                channel.set_onmessage(Some(handler.as_ref().unchecked_ref()));
                BROADCAST_HANDLER.with(|slot| {
                    slot.replace(Some(handler));
                });
                channel_cell.replace(Some(channel));
            }
            Err(err) => {
                log_warning("Failed to initialize BroadcastChannel", Some(&err));
            }
        }
    });
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
fn handle_broadcast_message(message: BroadcastMessage) {
    match message.payload {
        BroadcastPayload::Set(record) => cache_set(&message.app_key, Some(record)),
        BroadcastPayload::Remove => cache_set(&message.app_key, None),
    }
    notify_waiters(&message.app_key);
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
fn broadcast_update(app_key: &str, payload: BroadcastPayload) {
    BROADCAST_CHANNEL.with(|cell| {
        if cell.borrow().is_none() {
            ensure_broadcast_channel();
        }
    });

    BROADCAST_CHANNEL.with(|cell| {
        if let Some(channel) = cell.borrow().as_ref() {
            let message = BroadcastMessage {
                app_key: app_key.to_string(),
                payload,
            };
            if let Ok(serialized) = serde_json::to_string(&message) {
                if let Err(err) = channel.post_message(&JsValue::from_str(&serialized)) {
                    log_warning("Failed to broadcast messaging token update", Some(&err));
                }
            }
        }
    });
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
fn log_warning(message: &str, err: Option<&JsValue>) {
    if let Some(err) = err {
        web_sys::console::warn_2(&JsValue::from_str(message), err);
    } else {
        web_sys::console::warn_1(&JsValue::from_str(message));
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
fn notify_waiters(app_key: &str) {
    TOKEN_WAITERS.with(|waiters| {
        let mut waiters = waiters.borrow_mut();
        let mut idx = 0;
        while idx < waiters.len() {
            if waiters[idx].app_key == app_key {
                let waiter = waiters.remove(idx);
                let _ = waiter.sender.send(());
            } else {
                idx += 1;
            }
        }
    });
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
fn cleanup_waiters() {
    TOKEN_WAITERS.with(|waiters| {
        waiters.borrow_mut().retain(|waiter| !waiter.sender.is_canceled());
    });
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
fn register_waiter(app_key: &str) -> futures::channel::oneshot::Receiver<()> {
    let (sender, receiver) = futures::channel::oneshot::channel();
    TOKEN_WAITERS.with(|waiters| {
        waiters.borrow_mut().push(TokenWaiter {
            app_key: app_key.to_string(),
            sender,
        });
    });
    receiver
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
pub async fn wait_for_token_update(app_key: &str, timeout_ms: u64) -> MessagingResult<Option<TokenRecord>> {
    use std::time::Duration;

    ensure_broadcast_channel();
    let receiver = register_waiter(app_key);
    let _ = crate::platform::runtime::with_timeout(receiver, Duration::from_millis(timeout_ms)).await;
    cleanup_waiters();
    Ok(cache_get(app_key).flatten())
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
pub struct RefreshLock {
    key: String,
    owner: String,
    acquired: bool,
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
impl RefreshLock {
    pub fn new(app_key: &str) -> Self {
        let owner = format!("{}-{}", js_sys::Date::now(), (js_sys::Math::random() * 1_000_000.0) as u64);
        Self {
            key: format!("{REFRESH_LOCK_PREFIX}{app_key}"),
            owner,
            acquired: false,
        }
    }

    pub fn try_acquire(&mut self, ttl_ms: u64) -> bool {
        self.acquired = false;
        let window = match web_sys::window() {
            Some(window) => window,
            None => {
                self.acquired = true;
                return true;
            }
        };
        let storage = match window.local_storage() {
            Ok(Some(storage)) => storage,
            _ => {
                self.acquired = true;
                return true;
            }
        };

        let now = js_sys::Date::now() as u64;
        if let Ok(Some(raw)) = storage.get_item(&self.key) {
            if let Ok(existing) = serde_json::from_str::<RefreshLockState>(&raw) {
                if now < existing.timestamp_ms.saturating_add(ttl_ms) {
                    return false;
                }
            }
        }

        let state = RefreshLockState {
            owner: self.owner.clone(),
            timestamp_ms: now,
        };
        if let Ok(serialized) = serde_json::to_string(&state) {
            let _ = storage.set_item(&self.key, &serialized);
            self.acquired = true;
            true
        } else {
            false
        }
    }

    pub fn release(&mut self) {
        if !self.acquired {
            return;
        }

        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                if let Ok(Some(raw)) = storage.get_item(&self.key) {
                    if let Ok(existing) = serde_json::from_str::<RefreshLockState>(&raw) {
                        if existing.owner == self.owner {
                            let _ = storage.remove_item(&self.key);
                        }
                    }
                }
            }
        }

        self.acquired = false;
    }

    pub fn is_acquired(&self) -> bool {
        self.acquired
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
impl Drop for RefreshLock {
    fn drop(&mut self) {
        self.release();
    }
}
