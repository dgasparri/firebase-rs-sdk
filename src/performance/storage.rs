use std::collections::VecDeque;

#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_lock::Mutex;

use serde::{Deserialize, Serialize};

use crate::app::FirebaseApp;
use crate::performance::api::{HttpMethod, NetworkRequestRecord, PerformanceTrace};
#[cfg(any(not(target_arch = "wasm32"), feature = "experimental-indexed-db"))]
use crate::performance::error::internal_error;

use crate::performance::error::PerformanceResult;

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
use crate::platform::browser::indexed_db;

#[derive(Clone, Debug)]
pub enum TraceEnvelope {
    Trace(PerformanceTrace),
    Network(NetworkRequestRecord),
}

impl TraceEnvelope {}

#[cfg_attr(all(target_arch = "wasm32"), async_trait::async_trait(?Send))]
#[cfg_attr(not(all(target_arch = "wasm32")), async_trait::async_trait)]
pub trait TraceStore: Send + Sync {
    async fn push(&self, envelope: TraceEnvelope) -> PerformanceResult<()>;
    async fn drain(&self, max: usize) -> PerformanceResult<Vec<TraceEnvelope>>;
}

#[cfg(target_arch = "wasm32")]
pub type TraceStoreHandle = Arc<dyn TraceStore>;

#[cfg(not(target_arch = "wasm32"))]
pub type TraceStoreHandle = Arc<dyn TraceStore + Send + Sync>;

pub fn create_trace_store(_app: &FirebaseApp) -> TraceStoreHandle {
    #[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
    {
        return Arc::new(IndexedDbTraceStore::new(_app.name()));
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        return Arc::new(FileTraceStore::new(_app.name()));
    }

    #[allow(unreachable_code)]
    Arc::new(InMemoryTraceStore::default())
}

#[derive(Default, Clone)]
struct InMemoryTraceStore {
    queue: Arc<Mutex<VecDeque<TraceEnvelope>>>,
}

#[cfg_attr(all(target_arch = "wasm32"), async_trait::async_trait(?Send))]
#[cfg_attr(not(all(target_arch = "wasm32")), async_trait::async_trait)]
impl TraceStore for InMemoryTraceStore {
    async fn push(&self, envelope: TraceEnvelope) -> PerformanceResult<()> {
        self.queue.lock().await.push_back(envelope);
        Ok(())
    }

    async fn drain(&self, max: usize) -> PerformanceResult<Vec<TraceEnvelope>> {
        let mut guard = self.queue.lock().await;
        let mut drained = Vec::new();
        for _ in 0..max {
            if let Some(entry) = guard.pop_front() {
                drained.push(entry);
            } else {
                break;
            }
        }
        Ok(drained)
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct FileTraceStore {
    queue: Mutex<VecDeque<TraceEnvelope>>,
    path: PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl FileTraceStore {
    fn new(app_name: &str) -> Self {
        let path = storage_path(app_name);
        let queue = Mutex::new(load_queue(&path).unwrap_or_default());
        Self { queue, path }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg_attr(all(target_arch = "wasm32"), async_trait::async_trait(?Send))]
#[cfg_attr(not(all(target_arch = "wasm32")), async_trait::async_trait)]
impl TraceStore for FileTraceStore {
    async fn push(&self, envelope: TraceEnvelope) -> PerformanceResult<()> {
        let mut guard = self.queue.lock().await;
        guard.push_back(envelope);
        persist_queue(&self.path, &guard)
    }

    async fn drain(&self, max: usize) -> PerformanceResult<Vec<TraceEnvelope>> {
        let mut guard = self.queue.lock().await;
        let mut drained = Vec::new();
        for _ in 0..max {
            if let Some(entry) = guard.pop_front() {
                drained.push(entry);
            } else {
                break;
            }
        }
        persist_queue(&self.path, &guard)?;
        Ok(drained)
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
struct IndexedDbTraceStore {
    db_name: String,
    store_name: String,
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
impl IndexedDbTraceStore {
    fn new(app_name: &str) -> Self {
        Self {
            db_name: format!("firebase-perf-{}", app_name),
            store_name: "trace-queue".to_string(),
        }
    }

    async fn load_queue(&self) -> PerformanceResult<VecDeque<TraceEnvelope>> {
        let db = indexed_db::open_database_with_store(&self.db_name, 1, &self.store_name)
            .await
            .map_err(|err| internal_error(err.to_string()))?;
        match indexed_db::get_string(&db, &self.store_name, "queue").await {
            Ok(Some(raw)) => deserialize_queue(&raw),
            Ok(None) => Ok(VecDeque::new()),
            Err(err) => Err(internal_error(err.to_string())),
        }
    }

    async fn store_queue(&self, queue: &VecDeque<TraceEnvelope>) -> PerformanceResult<()> {
        let db = indexed_db::open_database_with_store(&self.db_name, 1, &self.store_name)
            .await
            .map_err(|err| internal_error(err.to_string()))?;
        let payload = serialize_queue(queue)?;
        indexed_db::put_string(&db, &self.store_name, "queue", &payload)
            .await
            .map_err(|err| internal_error(err.to_string()))
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32", feature = "experimental-indexed-db"))]
#[cfg_attr(all(target_arch = "wasm32"), async_trait::async_trait(?Send))]
#[cfg_attr(not(all(target_arch = "wasm32")), async_trait::async_trait)]
impl TraceStore for IndexedDbTraceStore {
    async fn push(&self, envelope: TraceEnvelope) -> PerformanceResult<()> {
        let mut queue = self.load_queue().await?;
        queue.push_back(envelope);
        self.store_queue(&queue).await
    }

    async fn drain(&self, max: usize) -> PerformanceResult<Vec<TraceEnvelope>> {
        let mut queue = self.load_queue().await?;
        let mut drained = Vec::new();
        for _ in 0..max {
            if let Some(entry) = queue.pop_front() {
                drained.push(entry);
            } else {
                break;
            }
        }
        self.store_queue(&queue).await?;
        Ok(drained)
    }
}

#[cfg(any(not(target_arch = "wasm32"), feature = "experimental-indexed-db"))]
fn serialize_queue(queue: &VecDeque<TraceEnvelope>) -> PerformanceResult<String> {
    let serialized: Vec<SerializableTraceEnvelope> = queue.iter().map(SerializableTraceEnvelope::from).collect();
    serde_json::to_string(&serialized).map_err(|err| internal_error(err.to_string()))
}

#[cfg(any(not(target_arch = "wasm32"), feature = "experimental-indexed-db"))]
fn deserialize_queue(raw: &str) -> PerformanceResult<VecDeque<TraceEnvelope>> {
    let mut queue = VecDeque::new();
    let serialized: Vec<SerializableTraceEnvelope> =
        serde_json::from_str(raw).map_err(|err| internal_error(err.to_string()))?;
    for item in serialized {
        queue.push_back(item.try_into()?);
    }
    Ok(queue)
}

#[cfg(not(target_arch = "wasm32"))]
fn storage_path(app_name: &str) -> PathBuf {
    if let Ok(dir) = std::env::var("FIREBASE_PERF_STORAGE_DIR") {
        return Path::new(&dir).join(format!("{app_name}-perf.json"));
    }
    let mut base = std::env::temp_dir();
    base.push(format!("firebase-perf-{app_name}.json"));
    base
}

#[cfg(not(target_arch = "wasm32"))]
fn load_queue(path: &Path) -> PerformanceResult<VecDeque<TraceEnvelope>> {
    if !path.exists() {
        return Ok(VecDeque::new());
    }
    let raw = fs::read_to_string(path).map_err(|err| internal_error(err.to_string()))?;
    deserialize_queue(&raw)
}

#[cfg(not(target_arch = "wasm32"))]
fn persist_queue(path: &Path, queue: &VecDeque<TraceEnvelope>) -> PerformanceResult<()> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let payload = serialize_queue(queue)?;
    fs::write(path, payload).map_err(|err| internal_error(err.to_string()))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SerializableTrace {
    name: String,
    start_time_us: u128,
    duration_us: u64,
    metrics: std::collections::HashMap<String, i64>,
    attributes: std::collections::HashMap<String, String>,
    is_auto: bool,
    auth_uid: Option<String>,
}

impl From<&PerformanceTrace> for SerializableTrace {
    fn from(trace: &PerformanceTrace) -> Self {
        Self {
            name: trace.name.to_string(),
            start_time_us: trace.start_time_us,
            duration_us: duration_to_u64(trace.duration),
            metrics: trace.metrics.clone(),
            attributes: trace.attributes.clone(),
            is_auto: trace.is_auto,
            auth_uid: trace.auth_uid.clone(),
        }
    }
}

impl TryFrom<SerializableTrace> for PerformanceTrace {
    type Error = crate::performance::error::PerformanceError;

    fn try_from(value: SerializableTrace) -> Result<Self, Self::Error> {
        Ok(PerformanceTrace {
            name: Arc::from(value.name),
            start_time_us: value.start_time_us,
            duration: std::time::Duration::from_micros(value.duration_us),
            metrics: value.metrics,
            attributes: value.attributes,
            is_auto: value.is_auto,
            auth_uid: value.auth_uid,
        })
    }
}

fn duration_to_u64(duration: Duration) -> u64 {
    duration.as_micros().min(u64::MAX as u128) as u64
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SerializableNetworkRequest {
    url: String,
    http_method: HttpMethod,
    start_time_us: u128,
    time_to_request_completed_us: u128,
    time_to_response_initiated_us: Option<u128>,
    time_to_response_completed_us: Option<u128>,
    request_payload_bytes: Option<u64>,
    response_payload_bytes: Option<u64>,
    response_code: Option<u16>,
    response_content_type: Option<String>,
    app_check_token: Option<String>,
}

impl From<&NetworkRequestRecord> for SerializableNetworkRequest {
    fn from(record: &NetworkRequestRecord) -> Self {
        Self {
            url: record.url.clone(),
            http_method: record.http_method.clone(),
            start_time_us: record.start_time_us,
            time_to_request_completed_us: record.time_to_request_completed_us,
            time_to_response_initiated_us: record.time_to_response_initiated_us,
            time_to_response_completed_us: record.time_to_response_completed_us,
            request_payload_bytes: record.request_payload_bytes,
            response_payload_bytes: record.response_payload_bytes,
            response_code: record.response_code,
            response_content_type: record.response_content_type.clone(),
            app_check_token: record.app_check_token.clone(),
        }
    }
}

impl TryFrom<SerializableNetworkRequest> for NetworkRequestRecord {
    type Error = crate::performance::error::PerformanceError;

    fn try_from(value: SerializableNetworkRequest) -> Result<Self, Self::Error> {
        Ok(NetworkRequestRecord {
            url: value.url,
            http_method: value.http_method,
            start_time_us: value.start_time_us,
            time_to_request_completed_us: value.time_to_request_completed_us,
            time_to_response_initiated_us: value.time_to_response_initiated_us,
            time_to_response_completed_us: value.time_to_response_completed_us,
            request_payload_bytes: value.request_payload_bytes,
            response_payload_bytes: value.response_payload_bytes,
            response_code: value.response_code,
            response_content_type: value.response_content_type,
            app_check_token: value.app_check_token,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub(crate) enum SerializableTraceEnvelope {
    Trace(SerializableTrace),
    Network(SerializableNetworkRequest),
}

impl From<&TraceEnvelope> for SerializableTraceEnvelope {
    fn from(envelope: &TraceEnvelope) -> Self {
        match envelope {
            TraceEnvelope::Trace(trace) => SerializableTraceEnvelope::Trace(trace.into()),
            TraceEnvelope::Network(record) => SerializableTraceEnvelope::Network(record.into()),
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::performance::api::PerformanceTrace;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    fn unique_dir() -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let mut dir = std::env::temp_dir();
        dir.push(format!("firebase-perf-storage-test-{}", COUNTER.fetch_add(1, Ordering::SeqCst)));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn sample_trace(name: &str) -> PerformanceTrace {
        PerformanceTrace {
            name: Arc::from(name.to_string()),
            start_time_us: 42,
            duration: Duration::from_millis(5),
            metrics: HashMap::new(),
            attributes: HashMap::new(),
            is_auto: false,
            auth_uid: None,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn file_store_persists_trace_records() {
        let dir = unique_dir();
        std::env::set_var("FIREBASE_PERF_STORAGE_DIR", &dir);
        let store = super::FileTraceStore::new("persist-test");
        let trace = sample_trace("persisted");
        store
            .push(TraceEnvelope::Trace(trace.clone()))
            .await
            .expect("push trace");
        drop(store);

        let store = super::FileTraceStore::new("persist-test");
        let drained = store.drain(10).await.expect("drain");
        assert!(matches!(&drained[0], TraceEnvelope::Trace(t) if t.name.as_ref() == "persisted"));
    }
}

impl TryFrom<SerializableTraceEnvelope> for TraceEnvelope {
    type Error = crate::performance::error::PerformanceError;

    fn try_from(value: SerializableTraceEnvelope) -> Result<Self, Self::Error> {
        match value {
            SerializableTraceEnvelope::Trace(trace) => Ok(TraceEnvelope::Trace(trace.try_into()?)),
            SerializableTraceEnvelope::Network(record) => Ok(TraceEnvelope::Network(record.try_into()?)),
        }
    }
}
