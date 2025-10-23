use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
use percent_encoding::{percent_encode, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};

use crate::installations::error::{internal_error, InstallationsResult};
use crate::installations::types::InstallationToken;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedAuthToken {
    token: String,
    expires_at_ms: u64,
}

impl PersistedAuthToken {
    pub fn from_runtime(token: &InstallationToken) -> InstallationsResult<Self> {
        let millis = system_time_to_millis(token.expires_at)?;
        Ok(Self {
            token: token.token.clone(),
            expires_at_ms: millis,
        })
    }

    pub fn into_runtime(self) -> InstallationToken {
        InstallationToken {
            token: self.token,
            expires_at: UNIX_EPOCH + Duration::from_millis(self.expires_at_ms),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedInstallation {
    pub fid: String,
    pub refresh_token: String,
    pub auth_token: PersistedAuthToken,
}

#[cfg_attr(
    all(feature = "wasm-web", target_arch = "wasm32"),
    async_trait::async_trait(?Send)
)]
#[cfg_attr(
    not(all(feature = "wasm-web", target_arch = "wasm32")),
    async_trait::async_trait
)]
pub trait InstallationsPersistence: Send + Sync {
    async fn read(&self, app_name: &str) -> InstallationsResult<Option<PersistedInstallation>>;
    async fn write(&self, app_name: &str, entry: &PersistedInstallation)
        -> InstallationsResult<()>;
    async fn clear(&self, app_name: &str) -> InstallationsResult<()>;

    async fn try_acquire_registration_lock(&self, app_name: &str) -> InstallationsResult<bool> {
        let _ = app_name;
        Ok(true)
    }

    async fn release_registration_lock(&self, app_name: &str) -> InstallationsResult<()> {
        let _ = app_name;
        Ok(())
    }
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
use std::fs;
#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
use std::path::PathBuf;
#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
use std::sync::Arc;

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
#[derive(Clone, Debug)]
pub struct FilePersistence {
    base_dir: Arc<PathBuf>,
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
impl FilePersistence {
    pub fn new(base_dir: PathBuf) -> InstallationsResult<Self> {
        fs::create_dir_all(&base_dir).map_err(|err| {
            internal_error(format!(
                "Failed to create installations cache directory '{}': {}",
                base_dir.display(),
                err
            ))
        })?;
        Ok(Self {
            base_dir: Arc::new(base_dir),
        })
    }

    pub fn default() -> InstallationsResult<Self> {
        if let Ok(dir) = std::env::var("FIREBASE_INSTALLATIONS_CACHE_DIR") {
            return Self::new(PathBuf::from(dir));
        }

        let dir = std::env::current_dir()
            .map_err(|err| internal_error(format!("Failed to obtain working directory: {}", err)))?
            .join(".firebase/installations");
        Self::new(dir)
    }

    fn file_for(&self, app_name: &str) -> PathBuf {
        let encoded = percent_encode(app_name.as_bytes(), NON_ALPHANUMERIC).to_string();
        self.base_dir.join(format!("{}.json", encoded))
    }
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
#[cfg_attr(
    all(feature = "wasm-web", target_arch = "wasm32"),
    async_trait::async_trait(?Send)
)]
#[cfg_attr(
    not(all(feature = "wasm-web", target_arch = "wasm32")),
    async_trait::async_trait
)]
impl InstallationsPersistence for FilePersistence {
    async fn read(&self, app_name: &str) -> InstallationsResult<Option<PersistedInstallation>> {
        let path = self.file_for(app_name);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path).map_err(|err| {
            internal_error(format!(
                "Failed to read installations cache '{}': {}",
                path.display(),
                err
            ))
        })?;
        let entry = serde_json::from_slice(&bytes).map_err(|err| {
            internal_error(format!(
                "Failed to parse installations cache '{}': {}",
                path.display(),
                err
            ))
        })?;
        Ok(Some(entry))
    }

    async fn write(
        &self,
        app_name: &str,
        entry: &PersistedInstallation,
    ) -> InstallationsResult<()> {
        let path = self.file_for(app_name);
        let bytes = serde_json::to_vec(entry).map_err(|err| {
            internal_error(format!(
                "Failed to serialize installations cache '{}': {}",
                path.display(),
                err
            ))
        })?;
        fs::write(&path, bytes).map_err(|err| {
            internal_error(format!(
                "Failed to write installations cache '{}': {}",
                path.display(),
                err
            ))
        })
    }

    async fn clear(&self, app_name: &str) -> InstallationsResult<()> {
        let path = self.file_for(app_name);
        if path.exists() {
            fs::remove_file(&path).map_err(|err| {
                internal_error(format!(
                    "Failed to delete installations cache '{}': {}",
                    path.display(),
                    err
                ))
            })?;
        }
        Ok(())
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
mod wasm_persistence {
    use super::{
        internal_error, InstallationsPersistence, InstallationsResult, PersistedInstallation,
    };
    use crate::platform::browser::indexed_db;
    use serde::{Deserialize, Serialize};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast, JsValue};
    use web_sys::{BroadcastChannel, MessageEvent};

    const DATABASE_NAME: &str = "firebase-installations-database";
    const DATABASE_VERSION: u32 = 1;
    const STORE_NAME: &str = "firebase-installations-store";
    const BROADCAST_CHANNEL: &str = "firebase-installations-updates";
    const PENDING_PREFIX: &str = "pending::";
    const PENDING_TIMEOUT_MS: u64 = 60_000;

    #[derive(Clone, Debug, Default)]
    pub struct IndexedDbPersistence;

    impl IndexedDbPersistence {
        pub fn new() -> Self {
            Self
        }
    }

    #[cfg_attr(all(feature = "wasm-web", target_arch = "wasm32"), async_trait::async_trait(?Send))]
    impl InstallationsPersistence for IndexedDbPersistence {
        async fn read(&self, app_name: &str) -> InstallationsResult<Option<PersistedInstallation>> {
            ensure_broadcast_channel();
            if let Some(cached) = cache_get(app_name) {
                return Ok(cached);
            }

            let db = open_db().await?;
            let stored = indexed_db::get_string(&db, STORE_NAME, app_name)
                .await
                .map_err(map_indexed_db_error)?;
            let entry = match stored {
                Some(json) => {
                    let parsed = serde_json::from_str(&json).map_err(|err| {
                        internal_error(format!("Failed to parse stored installation: {err}"))
                    })?;
                    Some(parsed)
                }
                None => None,
            };
            cache_set(app_name, entry.clone());
            Ok(entry)
        }

        async fn write(
            &self,
            app_name: &str,
            entry: &PersistedInstallation,
        ) -> InstallationsResult<()> {
            ensure_broadcast_channel();
            let json = serde_json::to_string(entry).map_err(|err| {
                internal_error(format!("Failed to serialize installation: {err}"))
            })?;
            let db = open_db().await?;
            indexed_db::put_string(&db, STORE_NAME, app_name, &json)
                .await
                .map_err(map_indexed_db_error)?;
            cache_set(app_name, Some(entry.clone()));
            broadcast_update(app_name, BroadcastPayload::Set(entry.clone()));
            Ok(())
        }

        async fn clear(&self, app_name: &str) -> InstallationsResult<()> {
            ensure_broadcast_channel();
            let db = open_db().await?;
            let existed = indexed_db::get_string(&db, STORE_NAME, app_name)
                .await
                .map_err(map_indexed_db_error)?
                .is_some();
            if existed {
                indexed_db::delete_key(&db, STORE_NAME, app_name)
                    .await
                    .map_err(map_indexed_db_error)?;
            }
            let pending_key = pending_key(app_name);
            let _ = indexed_db::delete_key(&db, STORE_NAME, &pending_key).await;
            cache_set(app_name, None);
            broadcast_update(app_name, BroadcastPayload::Remove);
            Ok(())
        }

        async fn try_acquire_registration_lock(&self, app_name: &str) -> InstallationsResult<bool> {
            ensure_broadcast_channel();
            let db = open_db().await?;
            let key = pending_key(app_name);
            let now = current_timestamp_ms();
            if let Some(raw) = indexed_db::get_string(&db, STORE_NAME, &key)
                .await
                .map_err(map_indexed_db_error)?
            {
                if let Ok(timestamp) = raw.parse::<u64>() {
                    if now.saturating_sub(timestamp) < PENDING_TIMEOUT_MS {
                        return Ok(false);
                    }
                }
            }

            indexed_db::put_string(&db, STORE_NAME, &key, &now.to_string())
                .await
                .map_err(map_indexed_db_error)?;
            Ok(true)
        }

        async fn release_registration_lock(&self, app_name: &str) -> InstallationsResult<()> {
            let db = open_db().await?;
            let key = pending_key(app_name);
            let _ = indexed_db::delete_key(&db, STORE_NAME, &key)
                .await
                .map_err(map_indexed_db_error)?;
            Ok(())
        }
    }

    #[derive(Serialize, Deserialize, Clone, Debug)]
    struct BroadcastMessage {
        app_name: String,
        payload: BroadcastPayload,
    }

    #[derive(Serialize, Deserialize, Clone, Debug)]
    enum BroadcastPayload {
        Set(PersistedInstallation),
        Remove,
    }

    thread_local! {
        static CACHE: RefCell<HashMap<String, Option<PersistedInstallation>>> = RefCell::new(HashMap::new());
        static CHANNEL: RefCell<Option<BroadcastChannel>> = RefCell::new(None);
        static HANDLER: RefCell<Option<Closure<dyn FnMut(MessageEvent)>>> = RefCell::new(None);
    }

    async fn open_db() -> InstallationsResult<web_sys::IdbDatabase> {
        indexed_db::open_database_with_store(DATABASE_NAME, DATABASE_VERSION, STORE_NAME)
            .await
            .map_err(map_indexed_db_error)
    }

    fn pending_key(app_name: &str) -> String {
        format!("{PENDING_PREFIX}{app_name}")
    }

    fn cache_get(app_name: &str) -> Option<Option<PersistedInstallation>> {
        CACHE.with(|cache| cache.borrow().get(app_name).cloned())
    }

    fn cache_set(app_name: &str, value: Option<PersistedInstallation>) {
        CACHE.with(|cache| {
            cache.borrow_mut().insert(app_name.to_string(), value);
        });
    }

    fn ensure_broadcast_channel() {
        CHANNEL.with(|cell| {
            if cell.borrow().is_some() {
                return;
            }
            match BroadcastChannel::new(BROADCAST_CHANNEL) {
                Ok(channel) => {
                    let handler = Closure::wrap(Box::new(|event: MessageEvent| {
                        if let Some(text) = event.data().as_string() {
                            if let Ok(message) = serde_json::from_str::<BroadcastMessage>(&text) {
                                handle_broadcast(message);
                            }
                        }
                    }) as Box<dyn FnMut(_)>);
                    channel.set_onmessage(Some(handler.as_ref().unchecked_ref()));
                    HANDLER.with(|slot| {
                        slot.replace(Some(handler));
                    });
                    cell.replace(Some(channel));
                }
                Err(err) => {
                    log_warning(
                        "Failed to initialise installations BroadcastChannel",
                        Some(&err),
                    );
                }
            }
        });
    }

    fn handle_broadcast(message: BroadcastMessage) {
        match message.payload {
            BroadcastPayload::Set(entry) => cache_set(&message.app_name, Some(entry)),
            BroadcastPayload::Remove => cache_set(&message.app_name, None),
        }
    }

    fn broadcast_update(app_name: &str, payload: BroadcastPayload) {
        CHANNEL.with(|cell| {
            if cell.borrow().is_none() {
                ensure_broadcast_channel();
            }
        });

        CHANNEL.with(|cell| {
            if let Some(channel) = cell.borrow().as_ref() {
                let message = BroadcastMessage {
                    app_name: app_name.to_string(),
                    payload,
                };
                if let Ok(serialized) = serde_json::to_string(&message) {
                    if let Err(err) = channel.post_message(&JsValue::from_str(&serialized)) {
                        log_warning("Failed to broadcast installations update", Some(&err));
                    }
                }
            }
        });
    }

    fn log_warning(message: &str, err: Option<&JsValue>) {
        if let Some(err) = err {
            web_sys::console::warn_2(&JsValue::from_str(message), err);
        } else {
            web_sys::console::warn_1(&JsValue::from_str(message));
        }
    }

    fn map_indexed_db_error<E: std::fmt::Display>(
        err: E,
    ) -> crate::installations::error::InstallationsError {
        internal_error(format!("IndexedDB error: {err}"))
    }

    fn current_timestamp_ms() -> u64 {
        js_sys::Date::now() as u64
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
pub use wasm_persistence::IndexedDbPersistence;

fn system_time_to_millis(time: SystemTime) -> InstallationsResult<u64> {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .map_err(|_| internal_error("Token expiration must be after UNIX epoch"))?;
    Ok(duration.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::installations::types::InstallationToken;
    use std::time::{Duration, SystemTime};

    #[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
    fn temp_dir() -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        let unique = format!("installations-persistence-{}", uuid());
        path.push(unique);
        path
    }

    #[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
    fn uuid() -> String {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        format!("{}", COUNTER.fetch_add(1, Ordering::SeqCst))
    }

    #[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
    #[tokio::test(flavor = "current_thread")]
    async fn file_persistence_round_trip() {
        let dir = temp_dir();
        let persistence = FilePersistence::new(dir.clone()).unwrap();
        let token = InstallationToken {
            token: "token".into(),
            expires_at: SystemTime::now() + Duration::from_secs(60),
        };
        let entry = PersistedInstallation {
            fid: "fid".into(),
            refresh_token: "refresh".into(),
            auth_token: PersistedAuthToken::from_runtime(&token).unwrap(),
        };

        persistence.write("app", &entry).await.unwrap();
        let loaded = persistence.read("app").await.unwrap().unwrap();
        assert_eq!(loaded, entry);

        persistence.clear("app").await.unwrap();
        assert!(persistence.read("app").await.unwrap().is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
