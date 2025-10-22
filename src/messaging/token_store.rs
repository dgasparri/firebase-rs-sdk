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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenRecord {
    pub token: String,
    pub create_time_ms: u64,
    pub subscription: Option<SubscriptionInfo>,
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
const DATABASE_NAME: &str = "firebase-messaging-database";
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
const DATABASE_VERSION: u32 = 1;
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
const STORE_NAME: &str = "firebase-messaging-store";

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
pub fn read_token(app_key: &str) -> MessagingResult<Option<TokenRecord>> {
    memory_store::read(app_key)
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
pub async fn read_token(app_key: &str) -> MessagingResult<Option<TokenRecord>> {
    let db = open_db().await?;
    let stored = indexed_db::get_string(&db, STORE_NAME, app_key)
        .await
        .map_err(|err| internal_error(err.to_string()))?;
    if let Some(json) = stored {
        let record = serde_json::from_str(&json)
            .map_err(|err| internal_error(format!("Failed to parse stored token: {err}")))?;
        Ok(Some(record))
    } else {
        Ok(None)
    }
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
pub fn write_token(app_key: &str, record: &TokenRecord) -> MessagingResult<()> {
    memory_store::write(app_key, record)
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
pub async fn write_token(app_key: &str, record: &TokenRecord) -> MessagingResult<()> {
    let json = serde_json::to_string(record)
        .map_err(|err| internal_error(format!("Failed to serialize token: {err}")))?;
    let db = open_db().await?;
    indexed_db::put_string(&db, STORE_NAME, app_key, &json)
        .await
        .map_err(|err| internal_error(err.to_string()))
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
pub fn remove_token(app_key: &str) -> MessagingResult<bool> {
    memory_store::remove(app_key)
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
pub async fn remove_token(app_key: &str) -> MessagingResult<bool> {
    let db = open_db().await?;
    let existed = indexed_db::get_string(&db, STORE_NAME, app_key)
        .await
        .map_err(|err| internal_error(err.to_string()))?
        .is_some();
    if existed {
        indexed_db::delete_key(&db, STORE_NAME, app_key)
            .await
            .map_err(|err| internal_error(err.to_string()))?;
    }
    Ok(existed)
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
async fn open_db() -> MessagingResult<web_sys::IdbDatabase> {
    indexed_db::open_database_with_store(DATABASE_NAME, DATABASE_VERSION, STORE_NAME)
        .await
        .map_err(|err| internal_error(err.to_string()))
}
