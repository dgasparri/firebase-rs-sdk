//! Remote Config storage cache and metadata handling.
//!
//! This mirrors the behaviour of the JavaScript `Storage` + `StorageCache` pair found in
//! `packages/remote-config/src/storage/`. The Rust implementation keeps everything in-process for now
//! but exposes a trait so a persistent backend can be plugged in later.

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::remote_config::constants::RC_CUSTOM_SIGNAL_MAX_ALLOWED_SIGNALS;

#[allow(unused_imports)]
use crate::remote_config::error::internal_error;
use crate::remote_config::error::{invalid_argument, RemoteConfigResult};

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

pub type CustomSignals = HashMap<String, JsonValue>;

/// Outcome of the last Remote Config fetch attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FetchStatus {
    NoFetchYet,
    Success,
    Failure,
    Throttle,
}

impl Default for FetchStatus {
    fn default() -> Self {
        FetchStatus::NoFetchYet
    }
}

impl FetchStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            FetchStatus::NoFetchYet => "no-fetch-yet",
            FetchStatus::Success => "success",
            FetchStatus::Failure => "failure",
            FetchStatus::Throttle => "throttle",
        }
    }
}

/// Abstraction over the persistence layer used to store Remote Config metadata.
///
/// A synchronous interface keeps usage ergonomic for the in-memory stub while still allowing
/// different backends to be introduced later on.
#[cfg_attr(
    all(feature = "wasm-web", target_arch = "wasm32"),
    async_trait::async_trait(?Send)
)]
#[cfg_attr(not(all(feature = "wasm-web", target_arch = "wasm32")), async_trait::async_trait)]
pub trait RemoteConfigStorage: Send + Sync {
    async fn get_last_fetch_status(&self) -> RemoteConfigResult<Option<FetchStatus>>;
    async fn set_last_fetch_status(&self, status: FetchStatus) -> RemoteConfigResult<()>;

    async fn get_last_successful_fetch_timestamp_millis(&self) -> RemoteConfigResult<Option<u64>>;
    async fn set_last_successful_fetch_timestamp_millis(&self, timestamp: u64) -> RemoteConfigResult<()>;

    async fn get_active_config(&self) -> RemoteConfigResult<Option<HashMap<String, String>>>;
    async fn set_active_config(&self, config: HashMap<String, String>) -> RemoteConfigResult<()>;

    async fn get_active_config_etag(&self) -> RemoteConfigResult<Option<String>>;
    async fn set_active_config_etag(&self, etag: Option<String>) -> RemoteConfigResult<()>;

    async fn get_active_config_template_version(&self) -> RemoteConfigResult<Option<u64>>;
    async fn set_active_config_template_version(&self, template_version: Option<u64>) -> RemoteConfigResult<()>;

    async fn get_custom_signals(&self) -> RemoteConfigResult<Option<CustomSignals>>;
    async fn set_custom_signals(&self, signals: CustomSignals) -> RemoteConfigResult<CustomSignals>;
}

/// In-memory storage backend backing the current stub implementation.
#[derive(Default)]
pub struct InMemoryRemoteConfigStorage {
    inner: Mutex<StorageRecord>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct StorageRecord {
    last_fetch_status: Option<FetchStatus>,
    last_successful_fetch_timestamp_millis: Option<u64>,
    active_config: Option<HashMap<String, String>>,
    active_config_etag: Option<String>,
    active_config_template_version: Option<u64>,
    custom_signals: Option<CustomSignals>,
}

#[cfg_attr(
    all(feature = "wasm-web", target_arch = "wasm32"),
    async_trait::async_trait(?Send)
)]
#[cfg_attr(not(all(feature = "wasm-web", target_arch = "wasm32")), async_trait::async_trait)]
impl RemoteConfigStorage for InMemoryRemoteConfigStorage {
    async fn get_last_fetch_status(&self) -> RemoteConfigResult<Option<FetchStatus>> {
        Ok(self.inner.lock().unwrap().last_fetch_status)
    }

    async fn set_last_fetch_status(&self, status: FetchStatus) -> RemoteConfigResult<()> {
        self.inner.lock().unwrap().last_fetch_status = Some(status);
        Ok(())
    }

    async fn get_last_successful_fetch_timestamp_millis(&self) -> RemoteConfigResult<Option<u64>> {
        Ok(self.inner.lock().unwrap().last_successful_fetch_timestamp_millis)
    }

    async fn set_last_successful_fetch_timestamp_millis(&self, timestamp: u64) -> RemoteConfigResult<()> {
        self.inner.lock().unwrap().last_successful_fetch_timestamp_millis = Some(timestamp);
        Ok(())
    }

    async fn get_active_config(&self) -> RemoteConfigResult<Option<HashMap<String, String>>> {
        Ok(self.inner.lock().unwrap().active_config.clone())
    }

    async fn set_active_config(&self, config: HashMap<String, String>) -> RemoteConfigResult<()> {
        self.inner.lock().unwrap().active_config = Some(config);
        Ok(())
    }

    async fn get_active_config_etag(&self) -> RemoteConfigResult<Option<String>> {
        Ok(self.inner.lock().unwrap().active_config_etag.clone())
    }

    async fn set_active_config_etag(&self, etag: Option<String>) -> RemoteConfigResult<()> {
        self.inner.lock().unwrap().active_config_etag = etag;
        Ok(())
    }

    async fn get_active_config_template_version(&self) -> RemoteConfigResult<Option<u64>> {
        Ok(self.inner.lock().unwrap().active_config_template_version)
    }

    async fn set_active_config_template_version(&self, template_version: Option<u64>) -> RemoteConfigResult<()> {
        self.inner.lock().unwrap().active_config_template_version = template_version;
        Ok(())
    }

    async fn get_custom_signals(&self) -> RemoteConfigResult<Option<CustomSignals>> {
        Ok(self.inner.lock().unwrap().custom_signals.clone())
    }

    async fn set_custom_signals(&self, signals: CustomSignals) -> RemoteConfigResult<CustomSignals> {
        let mut guard = self.inner.lock().unwrap();
        let merged = merge_custom_signals(guard.custom_signals.clone(), &signals)?;
        guard.custom_signals = Some(merged.clone());
        Ok(merged)
    }
}

/// Memory cache mirroring the JS SDK `StorageCache` abstraction.
pub struct RemoteConfigStorageCache {
    storage: Arc<dyn RemoteConfigStorage>,
    last_fetch_status: Mutex<FetchStatus>,
    last_successful_fetch_timestamp_millis: Mutex<Option<u64>>,
    active_config: Mutex<HashMap<String, String>>,
    active_config_etag: Mutex<Option<String>>,
    active_config_template_version: Mutex<Option<u64>>,
    custom_signals: Mutex<Option<CustomSignals>>,
}

/// File-backed Remote Config storage suitable for desktop environments.
#[cfg(not(target_arch = "wasm32"))]
pub struct FileRemoteConfigStorage {
    path: PathBuf,
    inner: Mutex<StorageRecord>,
}

impl fmt::Debug for RemoteConfigStorageCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoteConfigStorageCache")
            .field("last_fetch_status", &self.last_fetch_status())
            .field(
                "last_successful_fetch_timestamp_millis",
                &self.last_successful_fetch_timestamp_millis(),
            )
            .field("active_config_size", &self.active_config().len())
            .field("active_config_etag", &self.active_config_etag())
            .field(
                "custom_signals_count",
                &self.custom_signals().map(|signals| signals.len()).unwrap_or(0),
            )
            .finish()
    }
}

impl RemoteConfigStorageCache {
    pub fn new(storage: Arc<dyn RemoteConfigStorage>) -> Self {
        Self {
            storage,
            last_fetch_status: Mutex::new(FetchStatus::NoFetchYet),
            last_successful_fetch_timestamp_millis: Mutex::new(None),
            active_config: Mutex::new(HashMap::new()),
            active_config_etag: Mutex::new(None),
            active_config_template_version: Mutex::new(None),
            custom_signals: Mutex::new(None),
        }
    }

    pub async fn hydrate_from_storage(&self) -> RemoteConfigResult<()> {
        if let Some(status) = self.storage.get_last_fetch_status().await? {
            *self.last_fetch_status.lock().unwrap() = status;
        }
        if let Some(timestamp) = self.storage.get_last_successful_fetch_timestamp_millis().await? {
            *self.last_successful_fetch_timestamp_millis.lock().unwrap() = Some(timestamp);
        }
        if let Some(config) = self.storage.get_active_config().await? {
            *self.active_config.lock().unwrap() = config;
        }
        if let Some(etag) = self.storage.get_active_config_etag().await? {
            *self.active_config_etag.lock().unwrap() = Some(etag);
        }
        if let Some(template_version) = self.storage.get_active_config_template_version().await? {
            *self.active_config_template_version.lock().unwrap() = Some(template_version);
        }
        if let Some(signals) = self.storage.get_custom_signals().await? {
            *self.custom_signals.lock().unwrap() = Some(signals);
        }
        Ok(())
    }

    pub fn last_fetch_status(&self) -> FetchStatus {
        *self.last_fetch_status.lock().unwrap()
    }

    pub async fn set_last_fetch_status(&self, status: FetchStatus) -> RemoteConfigResult<()> {
        self.storage.set_last_fetch_status(status).await?;
        *self.last_fetch_status.lock().unwrap() = status;
        Ok(())
    }

    pub fn last_successful_fetch_timestamp_millis(&self) -> Option<u64> {
        *self.last_successful_fetch_timestamp_millis.lock().unwrap()
    }

    pub async fn set_last_successful_fetch_timestamp_millis(&self, timestamp: u64) -> RemoteConfigResult<()> {
        self.storage
            .set_last_successful_fetch_timestamp_millis(timestamp)
            .await?;
        *self.last_successful_fetch_timestamp_millis.lock().unwrap() = Some(timestamp);
        Ok(())
    }

    pub fn active_config(&self) -> HashMap<String, String> {
        self.active_config.lock().unwrap().clone()
    }

    pub async fn set_active_config(&self, config: HashMap<String, String>) -> RemoteConfigResult<()> {
        self.storage.set_active_config(config.clone()).await?;
        *self.active_config.lock().unwrap() = config;
        Ok(())
    }

    pub fn active_config_etag(&self) -> Option<String> {
        self.active_config_etag.lock().unwrap().clone()
    }

    pub async fn set_active_config_etag(&self, etag: Option<String>) -> RemoteConfigResult<()> {
        self.storage.set_active_config_etag(etag.clone()).await?;
        *self.active_config_etag.lock().unwrap() = etag;
        Ok(())
    }

    pub fn storage(&self) -> Arc<dyn RemoteConfigStorage> {
        Arc::clone(&self.storage)
    }

    pub fn active_config_template_version(&self) -> Option<u64> {
        *self.active_config_template_version.lock().unwrap()
    }

    pub async fn set_active_config_template_version(&self, template_version: Option<u64>) -> RemoteConfigResult<()> {
        self.storage
            .set_active_config_template_version(template_version)
            .await?;
        *self.active_config_template_version.lock().unwrap() = template_version;
        Ok(())
    }

    pub fn custom_signals(&self) -> Option<CustomSignals> {
        self.custom_signals.lock().unwrap().clone()
    }

    pub async fn set_custom_signals(&self, signals: CustomSignals) -> RemoteConfigResult<CustomSignals> {
        let merged = self.storage.set_custom_signals(signals).await?;
        *self.custom_signals.lock().unwrap() = Some(merged.clone());
        Ok(merged)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl FileRemoteConfigStorage {
    pub fn new(path: PathBuf) -> RemoteConfigResult<Self> {
        let record = if path.exists() {
            Self::load_record(&path)?
        } else {
            StorageRecord::default()
        };
        Ok(Self {
            path,
            inner: Mutex::new(record),
        })
    }

    fn load_record(path: &PathBuf) -> RemoteConfigResult<StorageRecord> {
        let data = fs::read(path).map_err(|err| internal_error(format!("failed to read storage file: {err}")))?;
        serde_json::from_slice(&data)
            .map_err(|err| internal_error(format!("failed to parse storage file as JSON: {err}")))
    }

    fn persist(&self, record: &StorageRecord) -> RemoteConfigResult<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| internal_error(format!("failed to create storage directory: {err}")))?;
        }
        let serialized = serde_json::to_vec_pretty(record)
            .map_err(|err| internal_error(format!("failed to serialize storage record: {err}")))?;
        fs::write(&self.path, serialized)
            .map_err(|err| internal_error(format!("failed to write storage file: {err}")))?;
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg_attr(
    all(feature = "wasm-web", target_arch = "wasm32"),
    async_trait::async_trait(?Send)
)]
#[cfg_attr(not(all(feature = "wasm-web", target_arch = "wasm32")), async_trait::async_trait)]
impl RemoteConfigStorage for FileRemoteConfigStorage {
    async fn get_last_fetch_status(&self) -> RemoteConfigResult<Option<FetchStatus>> {
        Ok(self.inner.lock().unwrap().last_fetch_status)
    }

    async fn set_last_fetch_status(&self, status: FetchStatus) -> RemoteConfigResult<()> {
        let mut record = self.inner.lock().unwrap();
        record.last_fetch_status = Some(status);
        self.persist(&record)
    }

    async fn get_last_successful_fetch_timestamp_millis(&self) -> RemoteConfigResult<Option<u64>> {
        Ok(self.inner.lock().unwrap().last_successful_fetch_timestamp_millis)
    }

    async fn set_last_successful_fetch_timestamp_millis(&self, timestamp: u64) -> RemoteConfigResult<()> {
        let mut record = self.inner.lock().unwrap();
        record.last_successful_fetch_timestamp_millis = Some(timestamp);
        self.persist(&record)
    }

    async fn get_active_config(&self) -> RemoteConfigResult<Option<HashMap<String, String>>> {
        Ok(self.inner.lock().unwrap().active_config.clone())
    }

    async fn set_active_config(&self, config: HashMap<String, String>) -> RemoteConfigResult<()> {
        let mut record = self.inner.lock().unwrap();
        record.active_config = Some(config);
        self.persist(&record)
    }

    async fn get_active_config_etag(&self) -> RemoteConfigResult<Option<String>> {
        Ok(self.inner.lock().unwrap().active_config_etag.clone())
    }

    async fn set_active_config_etag(&self, etag: Option<String>) -> RemoteConfigResult<()> {
        let mut record = self.inner.lock().unwrap();
        record.active_config_etag = etag;
        self.persist(&record)
    }

    async fn get_active_config_template_version(&self) -> RemoteConfigResult<Option<u64>> {
        Ok(self.inner.lock().unwrap().active_config_template_version)
    }

    async fn set_active_config_template_version(&self, template_version: Option<u64>) -> RemoteConfigResult<()> {
        let mut record = self.inner.lock().unwrap();
        record.active_config_template_version = template_version;
        self.persist(&record)
    }

    async fn get_custom_signals(&self) -> RemoteConfigResult<Option<CustomSignals>> {
        Ok(self.inner.lock().unwrap().custom_signals.clone())
    }

    async fn set_custom_signals(&self, signals: CustomSignals) -> RemoteConfigResult<CustomSignals> {
        let mut record = self.inner.lock().unwrap();
        let merged = merge_custom_signals(record.custom_signals.clone(), &signals)?;
        record.custom_signals = Some(merged.clone());
        self.persist(&record)?;
        Ok(merged)
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
#[derive(Clone, Debug, Default)]
pub struct IndexedDbRemoteConfigStorage;

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
impl IndexedDbRemoteConfigStorage {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
const REMOTE_CONFIG_DATABASE_NAME: &str = "firebase-remote-config";
#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
const REMOTE_CONFIG_DATABASE_VERSION: u32 = 1;
#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
const REMOTE_CONFIG_STORE_NAME: &str = "firebase-remote-config-store";

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
thread_local! {
    static REMOTE_CONFIG_DB_HANDLE: RefCell<Option<web_sys::IdbDatabase>> = RefCell::new(None);
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
const KEY_LAST_FETCH_STATUS: &str = "last_fetch_status";
#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
const KEY_LAST_SUCCESSFUL_FETCH_TIMESTAMP: &str = "last_successful_fetch_timestamp_millis";
#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
const KEY_ACTIVE_CONFIG: &str = "active_config";
#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
const KEY_ACTIVE_CONFIG_ETAG: &str = "active_config_etag";
#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
const KEY_ACTIVE_CONFIG_TEMPLATE_VERSION: &str = "active_config_template_version";
#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
const KEY_CUSTOM_SIGNALS: &str = "custom_signals";

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
async fn open_remote_config_db() -> RemoteConfigResult<web_sys::IdbDatabase> {
    if let Some(db) = REMOTE_CONFIG_DB_HANDLE.with(|cell| cell.borrow().clone()) {
        return Ok(db);
    }

    let db = crate::platform::browser::indexed_db::open_database_with_store(
        REMOTE_CONFIG_DATABASE_NAME,
        REMOTE_CONFIG_DATABASE_VERSION,
        REMOTE_CONFIG_STORE_NAME,
    )
    .await
    .map_err(|err| internal_error(err.to_string()))?;

    REMOTE_CONFIG_DB_HANDLE.with(|cell| {
        cell.replace(Some(db.clone()));
    });

    Ok(db)
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
async fn read_value<T>(key: &str) -> RemoteConfigResult<Option<T>>
where
    T: DeserializeOwned,
{
    let db = open_remote_config_db().await?;
    let stored = crate::platform::browser::indexed_db::get_string(&db, REMOTE_CONFIG_STORE_NAME, key)
        .await
        .map_err(|err| internal_error(err.to_string()))?;

    if let Some(raw) = stored {
        let value = serde_json::from_str(&raw)
            .map_err(|err| internal_error(format!("failed to parse stored value for '{key}': {err}")))?;
        Ok(Some(value))
    } else {
        Ok(None)
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
async fn write_value<T>(key: &str, value: Option<&T>) -> RemoteConfigResult<()>
where
    T: Serialize,
{
    let db = open_remote_config_db().await?;
    if let Some(value) = value {
        let serialized = serde_json::to_string(value)
            .map_err(|err| internal_error(format!("failed to serialize value for '{key}': {err}")))?;
        crate::platform::browser::indexed_db::put_string(&db, REMOTE_CONFIG_STORE_NAME, key, &serialized)
            .await
            .map_err(|err| internal_error(err.to_string()))?
    } else {
        crate::platform::browser::indexed_db::delete_key(&db, REMOTE_CONFIG_STORE_NAME, key)
            .await
            .map_err(|err| internal_error(err.to_string()))?
    }
    Ok(())
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
#[cfg_attr(
    all(feature = "wasm-web", target_arch = "wasm32"),
    async_trait::async_trait(?Send)
)]
impl RemoteConfigStorage for IndexedDbRemoteConfigStorage {
    async fn get_last_fetch_status(&self) -> RemoteConfigResult<Option<FetchStatus>> {
        read_value(KEY_LAST_FETCH_STATUS).await
    }

    async fn set_last_fetch_status(&self, status: FetchStatus) -> RemoteConfigResult<()> {
        write_value(KEY_LAST_FETCH_STATUS, Some(&status)).await
    }

    async fn get_last_successful_fetch_timestamp_millis(&self) -> RemoteConfigResult<Option<u64>> {
        read_value(KEY_LAST_SUCCESSFUL_FETCH_TIMESTAMP).await
    }

    async fn set_last_successful_fetch_timestamp_millis(&self, timestamp: u64) -> RemoteConfigResult<()> {
        write_value(KEY_LAST_SUCCESSFUL_FETCH_TIMESTAMP, Some(&timestamp)).await
    }

    async fn get_active_config(&self) -> RemoteConfigResult<Option<HashMap<String, String>>> {
        read_value(KEY_ACTIVE_CONFIG).await
    }

    async fn set_active_config(&self, config: HashMap<String, String>) -> RemoteConfigResult<()> {
        write_value(KEY_ACTIVE_CONFIG, Some(&config)).await
    }

    async fn get_active_config_etag(&self) -> RemoteConfigResult<Option<String>> {
        read_value(KEY_ACTIVE_CONFIG_ETAG).await
    }

    async fn set_active_config_etag(&self, etag: Option<String>) -> RemoteConfigResult<()> {
        write_value(KEY_ACTIVE_CONFIG_ETAG, etag.as_ref()).await
    }

    async fn get_active_config_template_version(&self) -> RemoteConfigResult<Option<u64>> {
        read_value(KEY_ACTIVE_CONFIG_TEMPLATE_VERSION).await
    }

    async fn set_active_config_template_version(&self, template_version: Option<u64>) -> RemoteConfigResult<()> {
        write_value(KEY_ACTIVE_CONFIG_TEMPLATE_VERSION, template_version.as_ref()).await
    }

    async fn get_custom_signals(&self) -> RemoteConfigResult<Option<CustomSignals>> {
        read_value(KEY_CUSTOM_SIGNALS).await
    }

    async fn set_custom_signals(&self, signals: CustomSignals) -> RemoteConfigResult<CustomSignals> {
        let existing = read_value::<CustomSignals>(KEY_CUSTOM_SIGNALS).await?;
        let merged = merge_custom_signals(existing, &signals)?;
        write_value(KEY_CUSTOM_SIGNALS, Some(&merged)).await?;
        Ok(merged)
    }
}

fn merge_custom_signals(existing: Option<CustomSignals>, updates: &CustomSignals) -> RemoteConfigResult<CustomSignals> {
    let mut merged = existing.unwrap_or_default();

    for (key, value) in updates {
        if value.is_null() {
            merged.remove(key);
            continue;
        }

        let normalized = match value {
            JsonValue::Number(number) => JsonValue::String(number.to_string()),
            other => other.clone(),
        };
        merged.insert(key.clone(), normalized);
    }

    if merged.len() > RC_CUSTOM_SIGNAL_MAX_ALLOWED_SIGNALS {
        return Err(invalid_argument(format!(
            "custom signals limit of {} exceeded",
            RC_CUSTOM_SIGNAL_MAX_ALLOWED_SIGNALS
        )));
    }

    Ok(merged)
}

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test(flavor = "current_thread")]
    async fn cache_roundtrips_metadata() {
        let storage: Arc<dyn RemoteConfigStorage> = Arc::new(InMemoryRemoteConfigStorage::default());
        let cache = RemoteConfigStorageCache::new(storage.clone());

        cache.hydrate_from_storage().await.unwrap();

        assert_eq!(cache.last_fetch_status(), FetchStatus::NoFetchYet);
        assert_eq!(cache.last_successful_fetch_timestamp_millis(), None);

        cache.set_last_fetch_status(FetchStatus::Success).await.unwrap();
        cache.set_last_successful_fetch_timestamp_millis(1234).await.unwrap();
        cache
            .set_active_config(HashMap::from([(String::from("feature"), String::from("on"))]))
            .await
            .unwrap();
        cache.set_active_config_etag(Some(String::from("etag"))).await.unwrap();
        cache.set_active_config_template_version(Some(42)).await.unwrap();
        cache
            .set_custom_signals(HashMap::from([(String::from("flag"), JsonValue::Bool(true))]))
            .await
            .unwrap();

        assert_eq!(cache.last_fetch_status(), FetchStatus::Success);
        assert_eq!(cache.last_successful_fetch_timestamp_millis(), Some(1234));
        let active = cache.active_config();
        assert_eq!(active.get("feature"), Some(&String::from("on")));
        assert_eq!(cache.active_config_etag(), Some(String::from("etag")));
        assert_eq!(cache.active_config_template_version(), Some(42));
        assert_eq!(
            cache.custom_signals().and_then(|signals| signals.get("flag").cloned()),
            Some(JsonValue::Bool(true))
        );

        // Creating a new cache on top of the same storage should hydrate state.
        let cache2 = RemoteConfigStorageCache::new(storage);
        cache2.hydrate_from_storage().await.unwrap();
        assert_eq!(cache2.last_fetch_status(), FetchStatus::Success);
        assert_eq!(cache2.last_successful_fetch_timestamp_millis(), Some(1234));
        assert_eq!(cache2.active_config().get("feature"), Some(&String::from("on")));
        assert_eq!(cache2.active_config_etag(), Some(String::from("etag")));
        assert_eq!(cache2.active_config_template_version(), Some(42));
        assert_eq!(
            cache2.custom_signals().and_then(|signals| signals.get("flag").cloned()),
            Some(JsonValue::Bool(true))
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn file_storage_persists_state() {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "firebase-remote-config-storage-{}.json",
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));

        let storage = Arc::new(FileRemoteConfigStorage::new(path.clone()).unwrap());
        let cache = RemoteConfigStorageCache::new(storage.clone());
        cache.hydrate_from_storage().await.unwrap();

        cache.set_last_fetch_status(FetchStatus::Success).await.unwrap();
        cache.set_last_successful_fetch_timestamp_millis(4321).await.unwrap();
        cache
            .set_active_config(HashMap::from([(String::from("color"), String::from("blue"))]))
            .await
            .unwrap();
        cache
            .set_active_config_etag(Some(String::from("persist-etag")))
            .await
            .unwrap();
        cache.set_active_config_template_version(Some(99)).await.unwrap();

        drop(cache);

        let storage2 = Arc::new(FileRemoteConfigStorage::new(path.clone()).unwrap());
        let cache2 = RemoteConfigStorageCache::new(storage2);
        cache2.hydrate_from_storage().await.unwrap();
        assert_eq!(cache2.last_fetch_status(), FetchStatus::Success);
        assert_eq!(cache2.last_successful_fetch_timestamp_millis(), Some(4321));
        assert_eq!(cache2.active_config().get("color"), Some(&"blue".into()));
        assert_eq!(cache2.active_config_etag(), Some(String::from("persist-etag")));
        assert_eq!(cache2.active_config_template_version(), Some(99));

        let _ = fs::remove_file(path);
    }
}
