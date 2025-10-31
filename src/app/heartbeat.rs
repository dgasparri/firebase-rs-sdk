use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use serde::Serialize;
use serde_json::json;

use async_lock::Mutex as AsyncMutex;
#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
use async_lock::OnceCell as AsyncOnceCell;

use crate::app::errors::AppResult;
#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
use crate::app::logger::LOGGER;
use crate::app::platform_logger::PlatformLoggerServiceImpl;
use crate::app::types::{
    FirebaseApp, HeartbeatService, HeartbeatStorage, HeartbeatsInStorage, PlatformLoggerService,
    SingleDateHeartbeat,
};
use crate::component::ComponentContainer;

#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
use crate::platform::browser::indexed_db;

const MAX_NUM_STORED_HEARTBEATS: usize = 30;
#[allow(dead_code)]
const MAX_HEADER_BYTES: usize = 1024;

pub struct HeartbeatServiceImpl {
    app: FirebaseApp,
    storage: Arc<dyn HeartbeatStorage>,
    cache: AsyncMutex<Option<HeartbeatsInStorage>>,
}

impl HeartbeatServiceImpl {
    /// Creates a heartbeat service tied to the given app and backing storage.
    pub fn new(app: FirebaseApp, storage: Arc<dyn HeartbeatStorage>) -> Self {
        Self {
            app,
            storage,
            cache: AsyncMutex::new(None),
        }
    }

    async fn load_cache(&self) -> AppResult<HeartbeatsInStorage> {
        {
            let guard = self.cache.lock().await;
            if let Some(cache) = guard.as_ref() {
                return Ok(cache.clone());
            }
        }

        let cache = self.storage.read().await?;
        let mut guard = self.cache.lock().await;
        *guard = Some(cache.clone());
        Ok(cache)
    }

    async fn update_cache(&self, value: HeartbeatsInStorage) {
        let mut guard = self.cache.lock().await;
        *guard = Some(value);
    }

    fn platform_agent(container: &ComponentContainer) -> Option<String> {
        container
            .get_provider("platform-logger")
            .get_immediate::<PlatformLoggerServiceImpl>()
            .map(|service| service.platform_info_string())
            .filter(|s| !s.is_empty())
    }

    fn today_utc() -> String {
        Utc::now().format("%Y-%m-%d").to_string()
    }

    fn prune_oldest(heartbeats: &mut Vec<SingleDateHeartbeat>) {
        if heartbeats.len() <= MAX_NUM_STORED_HEARTBEATS {
            return;
        }
        if let Some((index, _)) = heartbeats
            .iter()
            .enumerate()
            .min_by_key(|(_, hb)| hb.date.clone())
        {
            heartbeats.remove(index);
        }
    }

    #[allow(dead_code)]
    fn header_payload(heartbeats: &[SingleDateHeartbeat]) -> HeartbeatHeaderResult {
        let mut selected: Vec<HeartbeatsByUserAgent> = Vec::new();
        let mut unsent = Vec::new();

        for hb in heartbeats.iter().cloned() {
            let entry = selected
                .iter_mut()
                .find(|existing| existing.agent == hb.agent);
            if let Some(existing) = entry {
                existing.dates.push(hb.date.clone());
            } else {
                selected.push(HeartbeatsByUserAgent {
                    agent: hb.agent.clone(),
                    dates: vec![hb.date.clone()],
                });
            }

            if let Some(encoded) = encode_entries(&selected) {
                if encoded.len() <= MAX_HEADER_BYTES {
                    continue;
                }
            }

            if let Some(existing) = selected
                .iter_mut()
                .find(|existing| existing.agent == hb.agent)
            {
                existing.dates.pop();
                if existing.dates.is_empty() {
                    selected.retain(|entry| entry.agent != hb.agent);
                }
            }
            unsent.push(hb);
        }

        HeartbeatHeaderResult {
            heartbeats_to_send: selected,
            unsent,
        }
    }
}

#[cfg_attr(
    all(feature = "wasm-web", target_arch = "wasm32"),
    async_trait::async_trait(?Send)
)]
#[cfg_attr(
    not(all(feature = "wasm-web", target_arch = "wasm32")),
    async_trait::async_trait
)]
impl HeartbeatService for HeartbeatServiceImpl {
    async fn trigger_heartbeat(&self) -> AppResult<()> {
        let mut cache = self.load_cache().await?;
        let date = Self::today_utc();

        if cache.last_sent_heartbeat_date.as_deref() == Some(&date) {
            return Ok(());
        }

        if cache
            .heartbeats
            .iter()
            .any(|heartbeat| heartbeat.date == date)
        {
            return Ok(());
        }

        let agent =
            Self::platform_agent(&self.app.container()).unwrap_or_else(|| "unknown".to_string());
        cache.heartbeats.push(SingleDateHeartbeat { date, agent });
        Self::prune_oldest(&mut cache.heartbeats);
        self.storage.overwrite(&cache).await?;
        self.update_cache(cache).await;
        Ok(())
    }

    #[allow(dead_code)]
    async fn heartbeats_header(&self) -> AppResult<Option<String>> {
        let mut cache = self.load_cache().await?;
        if cache.heartbeats.is_empty() {
            return Ok(None);
        }

        let result = Self::header_payload(&cache.heartbeats);
        if result.heartbeats_to_send.is_empty() {
            return Ok(None);
        }

        let header = encode_entries(&result.heartbeats_to_send).unwrap_or_default();
        if header.is_empty() {
            return Ok(None);
        }

        cache.heartbeats = result.unsent;
        cache.last_sent_heartbeat_date = Some(Self::today_utc());
        self.storage.overwrite(&cache).await?;
        self.update_cache(cache).await;

        Ok(Some(header))
    }
}

pub struct InMemoryHeartbeatStorage {
    key: String,
}

impl InMemoryHeartbeatStorage {
    /// Builds an in-memory heartbeat store scoped to the provided app instance.
    pub fn new(app: &FirebaseApp) -> Self {
        let options = app.options();
        let key = format!("{}!{}", app.name(), options.app_id.unwrap_or_default());
        Self { key }
    }
}

#[cfg_attr(
    all(feature = "wasm-web", target_arch = "wasm32"),
    async_trait::async_trait(?Send)
)]
#[cfg_attr(
    not(all(feature = "wasm-web", target_arch = "wasm32")),
    async_trait::async_trait
)]
impl HeartbeatStorage for InMemoryHeartbeatStorage {
    async fn read(&self) -> AppResult<HeartbeatsInStorage> {
        let store = HEARTBEAT_STORE
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        Ok(store
            .get(&self.key)
            .cloned()
            .unwrap_or_else(HeartbeatsInStorage::default))
    }

    async fn overwrite(&self, value: &HeartbeatsInStorage) -> AppResult<()> {
        HEARTBEAT_STORE
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .insert(self.key.clone(), value.clone());
        Ok(())
    }
}

static HEARTBEAT_STORE: LazyLock<Mutex<HashMap<String, HeartbeatsInStorage>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[cfg(test)]
/// Clears any persisted heartbeat data in the in-memory store (test helper).
pub fn clear_heartbeat_store_for_tests() {
    HEARTBEAT_STORE
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clear();
}

pub(crate) fn storage_for_app(app: &FirebaseApp) -> Arc<dyn HeartbeatStorage> {
    #[cfg(all(
        feature = "wasm-web",
        target_arch = "wasm32",
        feature = "experimental-indexed-db"
    ))]
    {
        Arc::new(IndexedDbHeartbeatStorage::new(app.clone()))
    }

    #[cfg(not(all(
        feature = "wasm-web",
        target_arch = "wasm32",
        feature = "experimental-indexed-db"
    )))]
    {
        Arc::new(InMemoryHeartbeatStorage::new(app))
    }
}

#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
const HEARTBEAT_DB_NAME: &str = "firebase-heartbeat-database";
#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
const HEARTBEAT_DB_VERSION: u32 = 1;
#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
const HEARTBEAT_STORE_NAME: &str = "firebase-heartbeat-store";

#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
pub struct IndexedDbHeartbeatStorage {
    app: FirebaseApp,
    support: AsyncOnceCell<bool>,
}

#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
impl IndexedDbHeartbeatStorage {
    pub fn new(app: FirebaseApp) -> Self {
        Self {
            app,
            support: AsyncOnceCell::new(),
        }
    }

    fn key(&self) -> String {
        let options = self.app.options();
        format!("{}!{}", self.app.name(), options.app_id.unwrap_or_default())
    }

    async fn is_supported(&self) -> bool {
        *self
            .support
            .get_or_init(|| async {
                match indexed_db::open_database_with_store(
                    HEARTBEAT_DB_NAME,
                    HEARTBEAT_DB_VERSION,
                    HEARTBEAT_STORE_NAME,
                )
                .await
                {
                    Ok(db) => {
                        drop(db);
                        true
                    }
                    Err(err) => {
                        LOGGER.debug(format!(
                            "IndexedDB unavailable for heartbeat storage: {}",
                            format_indexed_db_error(&err)
                        ));
                        false
                    }
                }
            })
            .await
    }

    async fn read_inner(&self) -> Option<HeartbeatsInStorage> {
        let db = indexed_db::open_database_with_store(
            HEARTBEAT_DB_NAME,
            HEARTBEAT_DB_VERSION,
            HEARTBEAT_STORE_NAME,
        )
        .await
        .ok()?;
        match indexed_db::get_string(&db, HEARTBEAT_STORE_NAME, &self.key()).await {
            Ok(Some(payload)) => match serde_json::from_str::<HeartbeatsInStorage>(&payload) {
                Ok(value) => Some(value),
                Err(err) => {
                    LOGGER.warn(format!(
                        "Failed to decode heartbeats from IndexedDB: {}",
                        err
                    ));
                    None
                }
            },
            Ok(None) => None,
            Err(err) => {
                LOGGER.warn(format!(
                    "Failed to read heartbeats from IndexedDB: {}",
                    format_indexed_db_error(&err)
                ));
                None
            }
        }
    }

    async fn write_inner(&self, value: &HeartbeatsInStorage) {
        let json = match serde_json::to_string(value) {
            Ok(json) => json,
            Err(err) => {
                LOGGER.warn(format!(
                    "Failed to serialize heartbeats for IndexedDB: {}",
                    err
                ));
                return;
            }
        };

        match indexed_db::open_database_with_store(
            HEARTBEAT_DB_NAME,
            HEARTBEAT_DB_VERSION,
            HEARTBEAT_STORE_NAME,
        )
        .await
        {
            Ok(db) => {
                if let Err(err) =
                    indexed_db::put_string(&db, HEARTBEAT_STORE_NAME, &self.key(), &json).await
                {
                    LOGGER.warn(format!(
                        "Failed to write heartbeats to IndexedDB: {}",
                        format_indexed_db_error(&err)
                    ));
                }
            }
            Err(err) => {
                LOGGER.warn(format!(
                    "Failed to open IndexedDB for heartbeats: {}",
                    format_indexed_db_error(&err)
                ));
            }
        }
    }
}

#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
fn format_indexed_db_error(error: &indexed_db::Error) -> String {
    match error {
        indexed_db::Error::Unsupported(_) => "IndexedDB not supported".to_string(),
        indexed_db::Error::Operation(reason) => reason.clone(),
    }
}

#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
#[cfg_attr(
    all(feature = "wasm-web", target_arch = "wasm32"),
    async_trait::async_trait(?Send)
)]
#[cfg_attr(
    not(all(feature = "wasm-web", target_arch = "wasm32")),
    async_trait::async_trait
)]
impl HeartbeatStorage for IndexedDbHeartbeatStorage {
    async fn read(&self) -> AppResult<HeartbeatsInStorage> {
        if !self.is_supported().await {
            return Ok(HeartbeatsInStorage::default());
        }

        Ok(self.read_inner().await.unwrap_or_default())
    }

    async fn overwrite(&self, value: &HeartbeatsInStorage) -> AppResult<()> {
        if !self.is_supported().await {
            return Ok(());
        }

        let mut payload = value.clone();
        if payload.last_sent_heartbeat_date.is_none() {
            if let Some(existing) = self.read_inner().await {
                if payload.last_sent_heartbeat_date.is_none() {
                    payload.last_sent_heartbeat_date = existing.last_sent_heartbeat_date;
                }
            }
        }

        self.write_inner(&payload).await;
        Ok(())
    }
}

#[allow(dead_code)]
struct HeartbeatsByUserAgent {
    agent: String,
    dates: Vec<String>,
}

#[allow(dead_code)]
fn encode_entries(entries: &[HeartbeatsByUserAgent]) -> Option<String> {
    if entries.is_empty() {
        return None;
    }
    #[derive(Serialize)]
    struct HeartbeatEntry<'a> {
        agent: &'a str,
        dates: &'a [String],
    }

    let heartbeats: Vec<HeartbeatEntry<'_>> = entries
        .iter()
        .map(|entry| HeartbeatEntry {
            agent: entry.agent.as_str(),
            dates: &entry.dates,
        })
        .collect();

    let payload = json!({ "version": 2, "heartbeats": heartbeats });
    let serialized = serde_json::to_string(&payload).ok()?;
    Some(URL_SAFE_NO_PAD.encode(serialized))
}

#[allow(dead_code)]
struct HeartbeatHeaderResult {
    heartbeats_to_send: Vec<HeartbeatsByUserAgent>,
    unsent: Vec<SingleDateHeartbeat>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::types::{FirebaseAppConfig, FirebaseOptions};
    use crate::component::ComponentContainer;

    fn test_app(name: &str) -> FirebaseApp {
        let container = ComponentContainer::new(name);
        let options = FirebaseOptions {
            app_id: Some(format!("1:123:{}", name)),
            ..Default::default()
        };
        FirebaseApp::new(options, FirebaseAppConfig::new(name, true), container)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn heartbeat_header_returns_payload_and_resets_cache() {
        clear_heartbeat_store_for_tests();
        let app = test_app("heartbeat-test");
        let storage: Arc<dyn HeartbeatStorage> = Arc::new(InMemoryHeartbeatStorage::new(&app));
        let service = HeartbeatServiceImpl::new(app.clone(), storage);

        service
            .trigger_heartbeat()
            .await
            .expect("trigger heartbeat");
        let header = service.heartbeats_header().await.expect("header result");
        assert!(header.is_some(), "expected heartbeat header payload");

        // Subsequent call without new heartbeats should return None.
        let second = service
            .heartbeats_header()
            .await
            .expect("second header result");
        assert!(second.is_none());
    }
}
