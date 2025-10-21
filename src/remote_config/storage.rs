//! Remote Config storage cache and metadata handling.
//!
//! This mirrors the behaviour of the JavaScript `Storage` + `StorageCache` pair found in
//! `packages/remote-config/src/storage/`. The Rust implementation keeps everything in-process for now
//! but exposes a trait so a persistent backend can be plugged in later.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::remote_config::error::{internal_error, RemoteConfigResult};
use serde::{Deserialize, Serialize};

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
pub trait RemoteConfigStorage: Send + Sync {
    fn get_last_fetch_status(&self) -> RemoteConfigResult<Option<FetchStatus>>;
    fn set_last_fetch_status(&self, status: FetchStatus) -> RemoteConfigResult<()>;

    fn get_last_successful_fetch_timestamp_millis(&self) -> RemoteConfigResult<Option<u64>>;
    fn set_last_successful_fetch_timestamp_millis(&self, timestamp: u64) -> RemoteConfigResult<()>;

    fn get_active_config(&self) -> RemoteConfigResult<Option<HashMap<String, String>>>;
    fn set_active_config(&self, config: HashMap<String, String>) -> RemoteConfigResult<()>;

    fn get_active_config_etag(&self) -> RemoteConfigResult<Option<String>>;
    fn set_active_config_etag(&self, etag: Option<String>) -> RemoteConfigResult<()>;

    fn get_active_config_template_version(&self) -> RemoteConfigResult<Option<u64>>;
    fn set_active_config_template_version(
        &self,
        template_version: Option<u64>,
    ) -> RemoteConfigResult<()>;
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
}

impl RemoteConfigStorage for InMemoryRemoteConfigStorage {
    fn get_last_fetch_status(&self) -> RemoteConfigResult<Option<FetchStatus>> {
        Ok(self.inner.lock().unwrap().last_fetch_status)
    }

    fn set_last_fetch_status(&self, status: FetchStatus) -> RemoteConfigResult<()> {
        self.inner.lock().unwrap().last_fetch_status = Some(status);
        Ok(())
    }

    fn get_last_successful_fetch_timestamp_millis(&self) -> RemoteConfigResult<Option<u64>> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .last_successful_fetch_timestamp_millis)
    }

    fn set_last_successful_fetch_timestamp_millis(&self, timestamp: u64) -> RemoteConfigResult<()> {
        self.inner
            .lock()
            .unwrap()
            .last_successful_fetch_timestamp_millis = Some(timestamp);
        Ok(())
    }

    fn get_active_config(&self) -> RemoteConfigResult<Option<HashMap<String, String>>> {
        Ok(self.inner.lock().unwrap().active_config.clone())
    }

    fn set_active_config(&self, config: HashMap<String, String>) -> RemoteConfigResult<()> {
        self.inner.lock().unwrap().active_config = Some(config);
        Ok(())
    }

    fn get_active_config_etag(&self) -> RemoteConfigResult<Option<String>> {
        Ok(self.inner.lock().unwrap().active_config_etag.clone())
    }

    fn set_active_config_etag(&self, etag: Option<String>) -> RemoteConfigResult<()> {
        self.inner.lock().unwrap().active_config_etag = etag;
        Ok(())
    }

    fn get_active_config_template_version(&self) -> RemoteConfigResult<Option<u64>> {
        Ok(self.inner.lock().unwrap().active_config_template_version)
    }

    fn set_active_config_template_version(
        &self,
        template_version: Option<u64>,
    ) -> RemoteConfigResult<()> {
        self.inner.lock().unwrap().active_config_template_version = template_version;
        Ok(())
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
}

/// File-backed Remote Config storage suitable for desktop environments.
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
            .finish()
    }
}

impl RemoteConfigStorageCache {
    pub fn new(storage: Arc<dyn RemoteConfigStorage>) -> Self {
        let cache = Self {
            storage,
            last_fetch_status: Mutex::new(FetchStatus::NoFetchYet),
            last_successful_fetch_timestamp_millis: Mutex::new(None),
            active_config: Mutex::new(HashMap::new()),
            active_config_etag: Mutex::new(None),
            active_config_template_version: Mutex::new(None),
        };
        cache.load_from_storage();
        cache
    }

    fn load_from_storage(&self) {
        if let Ok(Some(status)) = self.storage.get_last_fetch_status() {
            *self.last_fetch_status.lock().unwrap() = status;
        }
        if let Ok(Some(timestamp)) = self.storage.get_last_successful_fetch_timestamp_millis() {
            *self.last_successful_fetch_timestamp_millis.lock().unwrap() = Some(timestamp);
        }
        if let Ok(Some(config)) = self.storage.get_active_config() {
            *self.active_config.lock().unwrap() = config;
        }
        if let Ok(Some(etag)) = self.storage.get_active_config_etag() {
            *self.active_config_etag.lock().unwrap() = Some(etag);
        }
        if let Ok(Some(template_version)) = self.storage.get_active_config_template_version() {
            *self.active_config_template_version.lock().unwrap() = Some(template_version);
        }
    }

    pub fn last_fetch_status(&self) -> FetchStatus {
        *self.last_fetch_status.lock().unwrap()
    }

    pub fn set_last_fetch_status(&self, status: FetchStatus) -> RemoteConfigResult<()> {
        self.storage.set_last_fetch_status(status)?;
        *self.last_fetch_status.lock().unwrap() = status;
        Ok(())
    }

    pub fn last_successful_fetch_timestamp_millis(&self) -> Option<u64> {
        *self.last_successful_fetch_timestamp_millis.lock().unwrap()
    }

    pub fn set_last_successful_fetch_timestamp_millis(
        &self,
        timestamp: u64,
    ) -> RemoteConfigResult<()> {
        self.storage
            .set_last_successful_fetch_timestamp_millis(timestamp)?;
        *self.last_successful_fetch_timestamp_millis.lock().unwrap() = Some(timestamp);
        Ok(())
    }

    pub fn active_config(&self) -> HashMap<String, String> {
        self.active_config.lock().unwrap().clone()
    }

    pub fn set_active_config(&self, config: HashMap<String, String>) -> RemoteConfigResult<()> {
        self.storage.set_active_config(config.clone())?;
        *self.active_config.lock().unwrap() = config;
        Ok(())
    }

    pub fn active_config_etag(&self) -> Option<String> {
        self.active_config_etag.lock().unwrap().clone()
    }

    pub fn set_active_config_etag(&self, etag: Option<String>) -> RemoteConfigResult<()> {
        self.storage.set_active_config_etag(etag.clone())?;
        *self.active_config_etag.lock().unwrap() = etag;
        Ok(())
    }

    pub fn storage(&self) -> Arc<dyn RemoteConfigStorage> {
        Arc::clone(&self.storage)
    }

    pub fn active_config_template_version(&self) -> Option<u64> {
        *self.active_config_template_version.lock().unwrap()
    }

    pub fn set_active_config_template_version(
        &self,
        template_version: Option<u64>,
    ) -> RemoteConfigResult<()> {
        self.storage
            .set_active_config_template_version(template_version)?;
        *self.active_config_template_version.lock().unwrap() = template_version;
        Ok(())
    }
}

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
        let data = fs::read(path)
            .map_err(|err| internal_error(format!("failed to read storage file: {err}")))?;
        serde_json::from_slice(&data)
            .map_err(|err| internal_error(format!("failed to parse storage file as JSON: {err}")))
    }

    fn persist(&self, record: &StorageRecord) -> RemoteConfigResult<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                internal_error(format!("failed to create storage directory: {err}"))
            })?;
        }
        let serialized = serde_json::to_vec_pretty(record)
            .map_err(|err| internal_error(format!("failed to serialize storage record: {err}")))?;
        fs::write(&self.path, serialized)
            .map_err(|err| internal_error(format!("failed to write storage file: {err}")))?;
        Ok(())
    }
}

impl RemoteConfigStorage for FileRemoteConfigStorage {
    fn get_last_fetch_status(&self) -> RemoteConfigResult<Option<FetchStatus>> {
        Ok(self.inner.lock().unwrap().last_fetch_status)
    }

    fn set_last_fetch_status(&self, status: FetchStatus) -> RemoteConfigResult<()> {
        let mut record = self.inner.lock().unwrap();
        record.last_fetch_status = Some(status);
        self.persist(&record)
    }

    fn get_last_successful_fetch_timestamp_millis(&self) -> RemoteConfigResult<Option<u64>> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .last_successful_fetch_timestamp_millis)
    }

    fn set_last_successful_fetch_timestamp_millis(&self, timestamp: u64) -> RemoteConfigResult<()> {
        let mut record = self.inner.lock().unwrap();
        record.last_successful_fetch_timestamp_millis = Some(timestamp);
        self.persist(&record)
    }

    fn get_active_config(&self) -> RemoteConfigResult<Option<HashMap<String, String>>> {
        Ok(self.inner.lock().unwrap().active_config.clone())
    }

    fn set_active_config(&self, config: HashMap<String, String>) -> RemoteConfigResult<()> {
        let mut record = self.inner.lock().unwrap();
        record.active_config = Some(config);
        self.persist(&record)
    }

    fn get_active_config_etag(&self) -> RemoteConfigResult<Option<String>> {
        Ok(self.inner.lock().unwrap().active_config_etag.clone())
    }

    fn set_active_config_etag(&self, etag: Option<String>) -> RemoteConfigResult<()> {
        let mut record = self.inner.lock().unwrap();
        record.active_config_etag = etag;
        self.persist(&record)
    }

    fn get_active_config_template_version(&self) -> RemoteConfigResult<Option<u64>> {
        Ok(self.inner.lock().unwrap().active_config_template_version)
    }

    fn set_active_config_template_version(
        &self,
        template_version: Option<u64>,
    ) -> RemoteConfigResult<()> {
        let mut record = self.inner.lock().unwrap();
        record.active_config_template_version = template_version;
        self.persist(&record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn cache_roundtrips_metadata() {
        let storage: Arc<dyn RemoteConfigStorage> =
            Arc::new(InMemoryRemoteConfigStorage::default());
        let cache = RemoteConfigStorageCache::new(storage.clone());

        assert_eq!(cache.last_fetch_status(), FetchStatus::NoFetchYet);
        assert_eq!(cache.last_successful_fetch_timestamp_millis(), None);

        cache.set_last_fetch_status(FetchStatus::Success).unwrap();
        cache
            .set_last_successful_fetch_timestamp_millis(1234)
            .unwrap();
        cache
            .set_active_config(HashMap::from([(
                String::from("feature"),
                String::from("on"),
            )]))
            .unwrap();
        cache
            .set_active_config_etag(Some(String::from("etag")))
            .unwrap();
        cache.set_active_config_template_version(Some(42)).unwrap();

        assert_eq!(cache.last_fetch_status(), FetchStatus::Success);
        assert_eq!(cache.last_successful_fetch_timestamp_millis(), Some(1234));
        let active = cache.active_config();
        assert_eq!(active.get("feature"), Some(&String::from("on")));
        assert_eq!(cache.active_config_etag(), Some(String::from("etag")));
        assert_eq!(cache.active_config_template_version(), Some(42));

        // Creating a new cache on top of the same storage should hydrate state.
        let cache2 = RemoteConfigStorageCache::new(storage);
        assert_eq!(cache2.last_fetch_status(), FetchStatus::Success);
        assert_eq!(cache2.last_successful_fetch_timestamp_millis(), Some(1234));
        assert_eq!(
            cache2.active_config().get("feature"),
            Some(&String::from("on"))
        );
        assert_eq!(cache2.active_config_etag(), Some(String::from("etag")));
        assert_eq!(cache2.active_config_template_version(), Some(42));
    }

    #[test]
    fn file_storage_persists_state() {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "firebase-remote-config-storage-{}.json",
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));

        let storage = Arc::new(FileRemoteConfigStorage::new(path.clone()).unwrap());
        let cache = RemoteConfigStorageCache::new(storage.clone());

        cache.set_last_fetch_status(FetchStatus::Success).unwrap();
        cache
            .set_last_successful_fetch_timestamp_millis(4321)
            .unwrap();
        cache
            .set_active_config(HashMap::from([(
                String::from("color"),
                String::from("blue"),
            )]))
            .unwrap();
        cache
            .set_active_config_etag(Some(String::from("persist-etag")))
            .unwrap();
        cache.set_active_config_template_version(Some(99)).unwrap();

        drop(cache);

        let storage2 = Arc::new(FileRemoteConfigStorage::new(path.clone()).unwrap());
        let cache2 = RemoteConfigStorageCache::new(storage2);
        assert_eq!(cache2.last_fetch_status(), FetchStatus::Success);
        assert_eq!(cache2.last_successful_fetch_timestamp_millis(), Some(4321));
        assert_eq!(cache2.active_config().get("color"), Some(&"blue".into()));
        assert_eq!(
            cache2.active_config_etag(),
            Some(String::from("persist-etag"))
        );
        assert_eq!(cache2.active_config_template_version(), Some(99));

        let _ = fs::remove_file(path);
    }
}
