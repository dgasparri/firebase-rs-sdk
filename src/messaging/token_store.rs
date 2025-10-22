use serde::{Deserialize, Serialize};

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use crate::messaging::error::internal_error;
use crate::messaging::error::MessagingResult;
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use crate::platform::browser::indexed_db;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscriptionInfo {
    pub vapid_key: String,
    pub scope: String,
    pub endpoint: String,
    pub auth: String,
    pub p256dh: String,
}

const AUTH_TOKEN_REFRESH_BUFFER_MS: u64 = 60_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenRecord {
    pub token: String,
    pub create_time_ms: u64,
    pub subscription: Option<SubscriptionInfo>,
    pub installation: InstallationInfo,
}

impl TokenRecord {
    #[cfg_attr(
        not(all(feature = "wasm-web", target_arch = "wasm32")),
        allow(dead_code)
    )]
    pub fn is_expired(&self, now_ms: u64, ttl_ms: u64) -> bool {
        now_ms.saturating_sub(self.create_time_ms) >= ttl_ms
    }

    #[cfg_attr(
        not(all(feature = "wasm-web", target_arch = "wasm32")),
        allow(dead_code)
    )]
    pub fn matches_subscription(&self, info: &SubscriptionInfo) -> bool {
        self.subscription.as_ref() == Some(info)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallationInfo {
    pub fid: String,
    pub refresh_token: String,
    pub auth_token: String,
    pub auth_token_expiration_ms: u64,
}

impl InstallationInfo {
    pub fn auth_token_expired(&self, now_ms: u64) -> bool {
        now_ms + AUTH_TOKEN_REFRESH_BUFFER_MS >= self.auth_token_expiration_ms
    }
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
mod memory_store {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use once_cell::sync::Lazy;

    use super::{MessagingResult, TokenRecord};

    static STORE: Lazy<Mutex<HashMap<String, TokenRecord>>> =
        Lazy::new(|| Mutex::new(HashMap::new()));

    pub fn read(app_key: &str) -> MessagingResult<Option<TokenRecord>> {
        Ok(STORE.lock().unwrap().get(app_key).cloned())
    }

    pub fn write(app_key: &str, record: &TokenRecord) -> MessagingResult<()> {
        STORE
            .lock()
            .unwrap()
            .insert(app_key.to_string(), record.clone());
        Ok(())
    }

    pub fn remove(app_key: &str) -> MessagingResult<bool> {
        Ok(STORE.lock().unwrap().remove(app_key).is_some())
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use std::cell::RefCell;
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use std::collections::HashMap;

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use wasm_bindgen::closure::Closure;
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use wasm_bindgen::{JsCast, JsValue};
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use web_sys::{BroadcastChannel, MessageEvent};

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
const DATABASE_NAME: &str = "firebase-messaging-database";
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
const DATABASE_VERSION: u32 = 1;
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
const STORE_NAME: &str = "firebase-messaging-store";
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
const BROADCAST_CHANNEL_NAME: &str = "firebase-messaging-token-updates";

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
#[derive(Serialize, Deserialize, Clone, Debug)]
struct BroadcastMessage {
    app_key: String,
    payload: BroadcastPayload,
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
#[derive(Serialize, Deserialize, Clone, Debug)]
enum BroadcastPayload {
    Set(TokenRecord),
    Remove,
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
thread_local! {
    static TOKEN_CACHE: RefCell<HashMap<String, Option<TokenRecord>>> = RefCell::new(HashMap::new());
    static BROADCAST_CHANNEL: RefCell<Option<BroadcastChannel>> = RefCell::new(None);
    static BROADCAST_HANDLER: RefCell<Option<Closure<dyn FnMut(MessageEvent)>>> = RefCell::new(None);
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
pub fn read_token(app_key: &str) -> MessagingResult<Option<TokenRecord>> {
    memory_store::read(app_key)
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
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

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
pub fn write_token(app_key: &str, record: &TokenRecord) -> MessagingResult<()> {
    memory_store::write(app_key, record)
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
pub async fn write_token(app_key: &str, record: &TokenRecord) -> MessagingResult<()> {
    ensure_broadcast_channel();
    let json = serde_json::to_string(record)
        .map_err(|err| internal_error(format!("Failed to serialize token: {err}")))?;
    let db = open_db().await?;
    indexed_db::put_string(&db, STORE_NAME, app_key, &json)
        .await
        .map_err(|err| internal_error(err.to_string()))?;
    cache_set(app_key, Some(record.clone()));
    broadcast_update(app_key, BroadcastPayload::Set(record.clone()));
    Ok(())
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
pub fn remove_token(app_key: &str) -> MessagingResult<bool> {
    memory_store::remove(app_key)
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
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
        broadcast_update(app_key, BroadcastPayload::Remove);
    }
    Ok(existed)
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
async fn open_db() -> MessagingResult<web_sys::IdbDatabase> {
    indexed_db::open_database_with_store(DATABASE_NAME, DATABASE_VERSION, STORE_NAME)
        .await
        .map_err(|err| internal_error(err.to_string()))
}


#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
fn cache_get(app_key: &str) -> Option<Option<TokenRecord>> {
    TOKEN_CACHE.with(|cache| cache.borrow().get(app_key).cloned())
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
fn cache_set(app_key: &str, value: Option<TokenRecord>) {
    TOKEN_CACHE.with(|cache| {
        cache.borrow_mut().insert(app_key.to_string(), value);
    });
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
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

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
fn handle_broadcast_message(message: BroadcastMessage) {
    match message.payload {
        BroadcastPayload::Set(record) => cache_set(&message.app_key, Some(record)),
        BroadcastPayload::Remove => cache_set(&message.app_key, None),
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
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

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
fn log_warning(message: &str, err: Option<&JsValue>) {
    if let Some(err) = err {
        web_sys::console::warn_2(&JsValue::from_str(message), err);
    } else {
        web_sys::console::warn_1(&JsValue::from_str(message));
    }
}
