//! Browser persistence helpers for App Check tokens using IndexedDB.

#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
pub use wasm::*;

#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
mod wasm {
    use std::sync::Arc;

    use js_sys::Math;
    use once_cell::sync::Lazy;
    use serde::{Deserialize, Serialize};
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use web_sys::{BroadcastChannel, MessageEvent};

    use crate::app_check::errors::{AppCheckError, AppCheckResult};
    use crate::app_check::types::AppCheckTokenResult;
    use crate::platform::browser::indexed_db::{
        delete_database, delete_key, get_string, open_database_with_store, put_string,
        IndexedDbError,
    };

    const DB_NAME: &str = "firebase-app-check";
    const STORE_NAME: &str = "app-check-store";
    const DB_VERSION: u32 = 1;
    const BROADCAST_CHANNEL: &str = "firebase-app-check-token";

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct PersistedToken {
        pub token: String,
        pub expire_time_ms: u64,
    }

    impl From<PersistedToken> for AppCheckTokenResult {
        fn from(value: PersistedToken) -> Self {
            AppCheckTokenResult {
                token: value.token,
                error: None,
                internal_error: None,
            }
        }
    }

    pub async fn load_token(app_id: &str) -> AppCheckResult<Option<PersistedToken>> {
        let db = open_database().await?;
        match get_string(&db, STORE_NAME, app_id).await {
            Ok(Some(json)) => {
                let parsed = serde_json::from_str(&json).map_err(|err| {
                    AppCheckError::Internal(format!(
                        "Failed to parse persisted App Check token: {err}"
                    ))
                })?;
                Ok(Some(parsed))
            }
            Ok(None) => Ok(None),
            Err(err) => Err(map_error(err)),
        }
    }

    pub async fn save_token(app_id: &str, token: &PersistedToken) -> AppCheckResult<()> {
        let db = open_database().await?;
        let json = serde_json::to_string(token).map_err(|err| {
            AppCheckError::Internal(format!(
                "Failed to serialize App Check token for storage: {err}"
            ))
        })?;
        put_string(&db, STORE_NAME, app_id, &json)
            .await
            .map_err(map_error)
            .map(|_| broadcast_update(app_id, Some(token)))
            .map(|_| ())
    }

    #[allow(dead_code)]
    pub async fn clear_token(app_id: &str) -> AppCheckResult<()> {
        let db = open_database().await?;
        delete_key(&db, STORE_NAME, app_id)
            .await
            .map_err(map_error)
            .map(|_| broadcast_update(app_id, None))
            .map(|_| ())
    }

    #[allow(dead_code)]
    pub async fn reset_persistence() -> AppCheckResult<()> {
        delete_database(DB_NAME).await.map_err(map_error)
    }

    async fn open_database() -> AppCheckResult<web_sys::IdbDatabase> {
        open_database_with_store(DB_NAME, DB_VERSION, STORE_NAME)
            .await
            .map_err(map_error)
    }

    fn map_error(error: IndexedDbError) -> AppCheckError {
        AppCheckError::Internal(error.to_string())
    }

    #[derive(Serialize, Deserialize)]
    struct BroadcastMessage {
        origin: String,
        app_id: String,
        token: Option<PersistedToken>,
    }

    fn broadcast_update(app_id: &str, token: Option<&PersistedToken>) {
        if let Ok(channel) = BroadcastChannel::new(BROADCAST_CHANNEL) {
            let message = BroadcastMessage {
                origin: INSTANCE_ID.clone(),
                app_id: app_id.to_string(),
                token: token.cloned(),
            };
            if let Ok(payload) = serde_json::to_string(&message) {
                let _ = channel.post_message(&JsValue::from_str(&payload));
            }
        }
    }

    static INSTANCE_ID: Lazy<String> = Lazy::new(|| format!("app-check-{}", Math::random()));

    pub fn subscribe(
        app_id: Arc<str>,
        callback: Arc<dyn Fn(Option<PersistedToken>) + Send + Sync>,
    ) -> Option<BroadcastSubscription> {
        let channel = BroadcastChannel::new(BROADCAST_CHANNEL).ok()?;
        let handler = Closure::wrap(Box::new(move |event: MessageEvent| {
            if let Some(text) = event.data().as_string() {
                if let Ok(message) = serde_json::from_str::<BroadcastMessage>(&text) {
                    if message.origin == *INSTANCE_ID {
                        return;
                    }
                    if message.app_id == *app_id {
                        callback(message.token);
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);
        Some(BroadcastSubscription::new(channel, handler))
    }

    #[allow(dead_code)]
    #[derive(Clone)]
    pub struct BroadcastSubscription {
        inner: Arc<BroadcastSubscriptionInner>,
    }

    #[allow(dead_code)]
    struct BroadcastSubscriptionInner {
        channel: BroadcastChannel,
        handler: Closure<dyn FnMut(MessageEvent)>,
    }

    impl BroadcastSubscription {
        fn new(channel: BroadcastChannel, handler: Closure<dyn FnMut(MessageEvent)>) -> Self {
            channel.set_onmessage(Some(handler.as_ref().unchecked_ref()));
            Self {
                inner: Arc::new(BroadcastSubscriptionInner { channel, handler }),
            }
        }
    }

    impl Drop for BroadcastSubscriptionInner {
        fn drop(&mut self) {
            self.channel.set_onmessage(None);
            let _ = self.channel.close();
        }
    }

    #[cfg(target_arch = "wasm32")]
    unsafe impl Send for BroadcastSubscription {}
    #[cfg(target_arch = "wasm32")]
    unsafe impl Sync for BroadcastSubscription {}

    #[cfg(all(
        test,
        feature = "wasm-web",
        feature = "experimental-indexed-db",
        target_arch = "wasm32"
    ))]
    mod tests {
        use super::*;
        use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

        wasm_bindgen_test_configure!(run_in_browser);

        fn token(value: &str, ttl_ms: u64) -> PersistedToken {
            PersistedToken {
                token: value.to_string(),
                expire_time_ms: ttl_ms,
            }
        }

        #[wasm_bindgen_test(async)]
        async fn roundtrip_token_persists_value() {
            reset_persistence().await.expect("reset persistence");

            save_token("app", &token("abc", 1234))
                .await
                .expect("save token");
            let loaded = load_token("app").await.expect("load token");
            assert!(loaded.is_some());
            let loaded = loaded.unwrap();
            assert_eq!(loaded.token, "abc");
            assert_eq!(loaded.expire_time_ms, 1234);

            clear_token("app").await.expect("clear token");
            let cleared = load_token("app").await.expect("load after clear");
            assert!(cleared.is_none());
        }
    }
}

#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    not(feature = "experimental-indexed-db")
))]
mod wasm_stub {
    #![allow(dead_code)]
    use std::sync::Arc;

    use crate::app_check::errors::AppCheckResult;
    use crate::app_check::types::AppCheckTokenResult;

    #[derive(Clone, Debug, Default)]
    pub struct PersistedToken {
        pub token: String,
        pub expire_time_ms: u64,
    }

    impl From<PersistedToken> for AppCheckTokenResult {
        fn from(value: PersistedToken) -> Self {
            AppCheckTokenResult {
                token: value.token,
                error: None,
                internal_error: None,
            }
        }
    }

    pub async fn load_token(_app_id: &str) -> AppCheckResult<Option<PersistedToken>> {
        Ok(None)
    }

    pub async fn save_token(_app_id: &str, _token: &PersistedToken) -> AppCheckResult<()> {
        Ok(())
    }

    pub async fn clear_token(_app_id: &str) -> AppCheckResult<()> {
        Ok(())
    }

    pub async fn reset_persistence() -> AppCheckResult<()> {
        Ok(())
    }

    #[allow(dead_code)]
    #[derive(Clone, Default)]
    pub struct BroadcastSubscription;

    #[cfg(target_arch = "wasm32")]
    unsafe impl Send for BroadcastSubscription {}
    #[cfg(target_arch = "wasm32")]
    unsafe impl Sync for BroadcastSubscription {}

    pub fn subscribe(
        _app_id: Arc<str>,
        _callback: Arc<dyn Fn(Option<PersistedToken>) + Send + Sync>,
    ) -> Option<BroadcastSubscription> {
        None
    }
}

#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    not(feature = "experimental-indexed-db")
))]
#[allow(unused_imports)]
pub use wasm_stub::*;

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
mod native {
    use crate::app_check::errors::AppCheckResult;
    use crate::app_check::types::AppCheckTokenResult;

    #[derive(Clone, Debug, Default)]
    pub struct PersistedToken {
        pub token: String,
        pub expire_time_ms: u64,
    }

    impl From<PersistedToken> for AppCheckTokenResult {
        fn from(value: PersistedToken) -> Self {
            AppCheckTokenResult {
                token: value.token,
                error: None,
                internal_error: None,
            }
        }
    }

    pub async fn load_token(_app_id: &str) -> AppCheckResult<Option<PersistedToken>> {
        Ok(None)
    }

    pub async fn save_token(_app_id: &str, _token: &PersistedToken) -> AppCheckResult<()> {
        Ok(())
    }

    pub async fn clear_token(_app_id: &str) -> AppCheckResult<()> {
        Ok(())
    }

    pub async fn reset_persistence() -> AppCheckResult<()> {
        Ok(())
    }
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
pub use native::*;
