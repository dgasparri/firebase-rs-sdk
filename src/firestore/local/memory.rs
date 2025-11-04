use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Debug, Formatter};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use async_lock::Mutex;
use async_trait::async_trait;

#[cfg(all(
    feature = "wasm-web",
    feature = "experimental-indexed-db",
    target_arch = "wasm32"
))]
use base64::Engine;

use crate::firestore::error::{invalid_argument, FirestoreError, FirestoreResult};
use crate::firestore::model::{DocumentKey, Timestamp};
use crate::firestore::remote::datastore::WriteOperation;
use crate::firestore::remote::mutation::{MutationBatch, MutationBatchResult};
use crate::firestore::remote::remote_event::RemoteEvent;
use crate::firestore::remote::streams::WriteResult;
use crate::firestore::remote::syncer_bridge::{
    RemoteSyncerBridge, RemoteSyncerDelegate, TargetMetadataUpdate,
};
use crate::firestore::remote::watch_change::WatchDocument;

#[derive(Clone, Debug, Default)]
pub struct TargetMetadataSnapshot {
    pub target_id: i32,
    pub resume_token: Option<Vec<u8>>,
    pub snapshot_version: Option<Timestamp>,
    pub current: bool,
    pub remote_keys: BTreeSet<DocumentKey>,
}

impl TargetMetadataSnapshot {
    fn new(target_id: i32) -> Self {
        Self {
            target_id,
            resume_token: None,
            snapshot_version: None,
            current: false,
            remote_keys: BTreeSet::new(),
        }
    }
}

pub trait LocalStorePersistence: Send + Sync {
    fn save_target_metadata(&self, _snapshot: TargetMetadataSnapshot) {}
    fn clear_target_metadata(&self, _target_id: i32) {}
    fn save_document_overlay(&self, _key: &DocumentKey, _overlay: &[WriteOperation]) {}
    fn clear_document_overlay(&self, _key: &DocumentKey) {}
}

/// In-memory store that mirrors the responsibilities of the Firestore JS LocalStore.
///
/// The implementation keeps per-target metadata (remote keys, resume tokens, snapshot
/// versions), overlays for pending writes, limbo bookkeeping, and a simple
/// persistence hook so higher-level tests can verify durability semantics without a
/// full persistence layer.
pub struct MemoryLocalStore {
    documents: Mutex<BTreeMap<DocumentKey, Option<WatchDocument>>>,
    remote_events: Mutex<Vec<RemoteEvent>>,
    rejected_targets: Mutex<Vec<(i32, FirestoreError)>>,
    successful_writes: Mutex<Vec<MutationBatchResult>>,
    failed_writes: Mutex<Vec<(i32, FirestoreError)>>,
    outstanding_batches: Mutex<Vec<i32>>,
    overlays: Mutex<BTreeMap<DocumentKey, Vec<WriteOperation>>>,
    next_batch_id: AtomicI32,

    last_stream_token: StdMutex<Option<Vec<u8>>>,
    write_results: StdMutex<Vec<(i32, Vec<WriteResult>)>>,
    target_metadata: StdMutex<BTreeMap<i32, TargetMetadataSnapshot>>,
    limbo_documents: StdMutex<BTreeSet<DocumentKey>>,
    persistence: Option<Arc<dyn LocalStorePersistence>>,
}

impl Debug for MemoryLocalStore {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryLocalStore").finish()
    }
}

impl MemoryLocalStore {
    pub fn new() -> Self {
        Self {
            documents: Mutex::new(BTreeMap::new()),
            remote_events: Mutex::new(Vec::new()),
            rejected_targets: Mutex::new(Vec::new()),
            successful_writes: Mutex::new(Vec::new()),
            failed_writes: Mutex::new(Vec::new()),
            outstanding_batches: Mutex::new(Vec::new()),
            overlays: Mutex::new(BTreeMap::new()),
            next_batch_id: AtomicI32::new(1),
            last_stream_token: StdMutex::new(None),
            write_results: StdMutex::new(Vec::new()),
            target_metadata: StdMutex::new(BTreeMap::new()),
            limbo_documents: StdMutex::new(BTreeSet::new()),
            persistence: None,
        }
    }

    pub fn new_with_persistence(persistence: Arc<dyn LocalStorePersistence>) -> Self {
        Self {
            persistence: Some(persistence),
            ..Self::new()
        }
    }

    #[cfg(all(
        feature = "wasm-web",
        feature = "experimental-indexed-db",
        target_arch = "wasm32"
    ))]
    pub fn new_with_indexed_db(db_name: impl Into<String>) -> Self {
        let persistence = IndexedDbPersistence::new(db_name);
        Self::new_with_persistence(Arc::new(persistence))
    }

    pub async fn queue_mutation_batch(
        &self,
        bridge: &RemoteSyncerBridge<Self>,
        writes: Vec<WriteOperation>,
    ) -> FirestoreResult<i32> {
        if writes.is_empty() {
            return Err(invalid_argument(
                "mutation batch must contain at least one write",
            ));
        }

        let batch_id = self.next_batch_id.fetch_add(1, Ordering::SeqCst);
        let batch = MutationBatch::from_writes(batch_id, Timestamp::now(), writes);
        bridge.enqueue_batch(batch.clone()).await?;
        self.outstanding_batches.lock().await.push(batch_id);

        let persistence = self.persistence.as_ref().map(Arc::clone);
        let mut overlay_snapshots = Vec::new();
        {
            let mut overlays = self.overlays.lock().await;
            for write in &batch.writes {
                let key = write.key().clone();
                let entry = overlays.entry(key.clone()).or_default();
                entry.push(write.clone());
                overlay_snapshots.push((key, entry.clone()));
            }
        }

        if let Some(persistence) = persistence {
            for (key, overlay) in overlay_snapshots {
                persistence.save_document_overlay(&key, &overlay);
            }
        }

        Ok(batch_id)
    }

    pub async fn replace_remote_keys(
        &self,
        bridge: &RemoteSyncerBridge<Self>,
        target_id: i32,
        keys: BTreeSet<DocumentKey>,
    ) {
        bridge.replace_remote_keys(target_id, keys.clone());
        let snapshot = {
            let mut targets = self.target_metadata.lock().unwrap();
            let entry = targets
                .entry(target_id)
                .or_insert_with(|| TargetMetadataSnapshot::new(target_id));
            entry.remote_keys = keys.clone();
            entry.clone()
        };

        if let Some(persistence) = &self.persistence {
            persistence.save_target_metadata(snapshot);
        }
    }

    pub async fn last_remote_event(&self) -> Option<RemoteEvent> {
        self.remote_events.lock().await.last().cloned()
    }

    pub async fn document(&self, key: &DocumentKey) -> Option<Option<WatchDocument>> {
        self.documents.lock().await.get(key).cloned()
    }

    pub async fn successful_batch_ids(&self) -> Vec<i32> {
        self.successful_writes
            .lock()
            .await
            .iter()
            .map(|result| result.batch_id())
            .collect()
    }

    pub async fn failed_batch_ids(&self) -> Vec<i32> {
        self.failed_writes
            .lock()
            .await
            .iter()
            .map(|(batch_id, _)| *batch_id)
            .collect()
    }

    pub async fn outstanding_batch_ids(&self) -> Vec<i32> {
        self.outstanding_batches.lock().await.clone()
    }

    pub fn last_stream_token(&self) -> Option<Vec<u8>> {
        self.last_stream_token.lock().unwrap().clone()
    }

    pub fn recorded_write_results(&self) -> Vec<(i32, Vec<WriteResult>)> {
        self.write_results.lock().unwrap().clone()
    }

    pub fn target_metadata_snapshot(&self, target_id: i32) -> Option<TargetMetadataSnapshot> {
        self.target_metadata
            .lock()
            .unwrap()
            .get(&target_id)
            .cloned()
    }

    pub fn limbo_documents_snapshot(&self) -> BTreeSet<DocumentKey> {
        self.limbo_documents.lock().unwrap().clone()
    }

    pub async fn overlays_snapshot(&self) -> BTreeMap<DocumentKey, Vec<WriteOperation>> {
        self.overlays.lock().await.clone()
    }

    pub fn track_limbo_document(&self, key: DocumentKey) {
        self.limbo_documents.lock().unwrap().insert(key);
    }

    async fn clear_all(&self) {
        let overlay_keys = {
            let mut documents = self.documents.lock().await;
            documents.clear();
            self.remote_events.lock().await.clear();
            self.rejected_targets.lock().await.clear();
            self.successful_writes.lock().await.clear();
            self.failed_writes.lock().await.clear();
            self.outstanding_batches.lock().await.clear();
            let mut overlays = self.overlays.lock().await;
            let keys = overlays.keys().cloned().collect::<Vec<_>>();
            overlays.clear();
            keys
        };

        if let Some(persistence) = &self.persistence {
            for key in overlay_keys {
                persistence.clear_document_overlay(&key);
            }
        }

        let cleared_targets = {
            let mut targets = self.target_metadata.lock().unwrap();
            let ids = targets.keys().copied().collect::<Vec<_>>();
            targets.clear();
            ids
        };

        if let Some(persistence) = &self.persistence {
            for target_id in cleared_targets {
                persistence.clear_target_metadata(target_id);
            }
        }

        self.limbo_documents.lock().unwrap().clear();

        if let Ok(mut token) = self.last_stream_token.lock() {
            *token = None;
        }
        if let Ok(mut results) = self.write_results.lock() {
            results.clear();
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl RemoteSyncerDelegate for MemoryLocalStore {
    async fn handle_remote_event(&self, event: RemoteEvent) -> FirestoreResult<()> {
        {
            let mut documents = self.documents.lock().await;
            for (key, maybe_doc) in &event.document_updates {
                match maybe_doc {
                    Some(doc) => {
                        documents.insert(key.clone(), Some(doc.clone()));
                    }
                    None => {
                        documents.insert(key.clone(), None);
                    }
                }
            }

            for key in &event.resolved_limbo_documents {
                documents.remove(key);
            }
        }

        self.remote_events.lock().await.push(event.clone());

        if !event.document_updates.is_empty() {
            let persistence = self.persistence.as_ref().map(Arc::clone);
            let keys: Vec<_> = event.document_updates.keys().cloned().collect();
            let mut overlays = self.overlays.lock().await;
            let cleared: Vec<_> = keys
                .into_iter()
                .filter(|key| overlays.remove(key).is_some())
                .collect();
            if let Some(persistence) = persistence {
                for key in cleared {
                    persistence.clear_document_overlay(&key);
                }
            }
        }

        if !event.resolved_limbo_documents.is_empty() {
            let mut limbo = self.limbo_documents.lock().unwrap();
            for key in &event.resolved_limbo_documents {
                limbo.remove(key);
            }
        }

        if let Some(snapshot) = event.snapshot_version {
            let persistence = self.persistence.as_ref().map(Arc::clone);
            let mut pending = Vec::new();
            {
                let mut targets = self.target_metadata.lock().unwrap();
                for (target_id, change) in &event.target_changes {
                    let entry = targets
                        .entry(*target_id)
                        .or_insert_with(|| TargetMetadataSnapshot::new(*target_id));
                    entry.snapshot_version = Some(snapshot);
                    if change.current {
                        entry.current = true;
                    }
                    pending.push(entry.clone());
                }
            }
            if let Some(persistence) = persistence {
                for snapshot in pending {
                    persistence.save_target_metadata(snapshot);
                }
            }
        }

        Ok(())
    }

    async fn handle_rejected_listen(
        &self,
        target_id: i32,
        error: FirestoreError,
    ) -> FirestoreResult<()> {
        self.rejected_targets.lock().await.push((target_id, error));
        Ok(())
    }

    async fn handle_successful_write(&self, result: MutationBatchResult) -> FirestoreResult<()> {
        let batch_id = result.batch_id();
        {
            let mut outstanding = self.outstanding_batches.lock().await;
            if let Some(pos) = outstanding.iter().position(|id| *id == batch_id) {
                outstanding.remove(pos);
            }
        }
        self.successful_writes.lock().await.push(result.clone());

        let persistence = self.persistence.as_ref().map(Arc::clone);
        let keys = result.batch.document_keys();
        if !keys.is_empty() {
            let mut overlays = self.overlays.lock().await;
            let cleared: Vec<_> = keys
                .into_iter()
                .filter(|key| overlays.remove(key).is_some())
                .collect();
            if let Some(persistence) = persistence {
                for key in cleared {
                    persistence.clear_document_overlay(&key);
                }
            }
        }
        Ok(())
    }

    async fn handle_failed_write(
        &self,
        batch_id: i32,
        error: FirestoreError,
    ) -> FirestoreResult<()> {
        {
            let mut outstanding = self.outstanding_batches.lock().await;
            if let Some(pos) = outstanding.iter().position(|id| *id == batch_id) {
                outstanding.remove(pos);
            }
        }
        self.failed_writes.lock().await.push((batch_id, error));

        let persistence = self.persistence.as_ref().map(Arc::clone);
        let mut overlays = self.overlays.lock().await;
        let cleared: Vec<_> = overlays.keys().cloned().collect();
        overlays.clear();
        if let Some(persistence) = persistence {
            for key in cleared {
                persistence.clear_document_overlay(&key);
            }
        }
        Ok(())
    }

    async fn handle_credential_change(&self) -> FirestoreResult<()> {
        self.clear_all().await;
        Ok(())
    }

    fn notify_stream_token_change(&self, token: Option<Vec<u8>>) {
        if let Ok(mut guard) = self.last_stream_token.lock() {
            *guard = token;
        }
    }

    fn record_write_results(&self, batch_id: i32, results: &[WriteResult]) {
        if let Ok(mut guard) = self.write_results.lock() {
            guard.push((batch_id, results.to_vec()));
        }
    }

    fn update_target_metadata(&self, target_id: i32, update: TargetMetadataUpdate) {
        let TargetMetadataUpdate {
            resume_token,
            snapshot_version,
            current,
            added_documents,
            modified_documents,
            removed_documents,
        } = update;

        let persistence = self.persistence.as_ref().map(Arc::clone);
        let snapshot = {
            let mut targets = self.target_metadata.lock().unwrap();
            let entry = targets
                .entry(target_id)
                .or_insert_with(|| TargetMetadataSnapshot::new(target_id));
            if let Some(token) = resume_token {
                if !token.is_empty() {
                    entry.resume_token = Some(token);
                }
            }
            if let Some(version) = snapshot_version {
                entry.snapshot_version = Some(version);
            }
            entry.current = current;
            for key in removed_documents {
                entry.remote_keys.remove(&key);
            }
            for key in added_documents
                .into_iter()
                .chain(modified_documents.into_iter())
            {
                entry.remote_keys.insert(key);
            }
            entry.clone()
        };

        if let Some(persistence) = persistence {
            persistence.save_target_metadata(snapshot);
        }
    }

    fn reset_target_metadata(&self, target_id: i32) {
        let persistence = self.persistence.as_ref().map(Arc::clone);
        let snapshot = {
            let mut targets = self.target_metadata.lock().unwrap();
            let entry = targets
                .entry(target_id)
                .or_insert_with(|| TargetMetadataSnapshot::new(target_id));
            entry.remote_keys.clear();
            entry.resume_token = None;
            entry.snapshot_version = None;
            entry.current = false;
            entry.clone()
        };

        if let Some(persistence) = persistence {
            persistence.clear_target_metadata(target_id);
            persistence.save_target_metadata(snapshot);
        }
    }

    fn record_resolved_limbo_documents(&self, documents: &BTreeSet<DocumentKey>) {
        let mut limbo = self.limbo_documents.lock().unwrap();
        for key in documents {
            limbo.remove(key);
        }
    }
}

#[cfg(all(
    feature = "wasm-web",
    feature = "experimental-indexed-db",
    target_arch = "wasm32"
))]
#[derive(Clone, Debug)]
struct IndexedDbPersistence {
    db_name: String,
    targets_store: String,
    overlays_store: String,
    version: u32,
}

#[cfg(all(
    feature = "wasm-web",
    feature = "experimental-indexed-db",
    target_arch = "wasm32"
))]
impl IndexedDbPersistence {
    fn new(db_name: impl Into<String>) -> Self {
        Self {
            db_name: db_name.into(),
            targets_store: "firestore_targets".into(),
            overlays_store: "firestore_overlays".into(),
            version: 1,
        }
    }

    fn spawn<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + 'static,
    {
        wasm_bindgen_futures::spawn_local(future);
    }

    async fn open_store(
        &self,
        store: &str,
    ) -> crate::platform::browser::indexed_db::IndexedDbResult<web_sys::IdbDatabase> {
        crate::platform::browser::indexed_db::open_database_with_store(
            &self.db_name,
            self.version,
            store,
        )
        .await
    }

    fn encode_target_snapshot(snapshot: &TargetMetadataSnapshot) -> String {
        let resume_token = snapshot
            .resume_token
            .as_ref()
            .map(|token| base64::engine::general_purpose::STANDARD.encode(token));
        let remote_keys: Vec<String> = snapshot
            .remote_keys
            .iter()
            .map(|key| key.path().canonical_string())
            .collect();
        let snapshot_version = snapshot.snapshot_version.map(|ts| {
            serde_json::json!({
                "seconds": ts.seconds,
                "nanos": ts.nanos,
            })
        });

        serde_json::json!({
            "targetId": snapshot.target_id,
            "resumeToken": resume_token,
            "snapshotVersion": snapshot_version,
            "current": snapshot.current,
            "remoteKeys": remote_keys,
        })
        .to_string()
    }

    fn encode_overlay(key: &DocumentKey, overlay: &[WriteOperation]) -> String {
        let write_paths: Vec<String> = overlay
            .iter()
            .map(|write| write.key().path().canonical_string())
            .collect();
        serde_json::json!({
            "key": key.path().canonical_string(),
            "writes": write_paths,
        })
        .to_string()
    }
}

#[cfg(all(
    feature = "wasm-web",
    feature = "experimental-indexed-db",
    target_arch = "wasm32"
))]
impl LocalStorePersistence for IndexedDbPersistence {
    fn save_target_metadata(&self, snapshot: TargetMetadataSnapshot) {
        let store = self.targets_store.clone();
        let db_name = self.db_name.clone();
        let version = self.version;
        let payload = Self::encode_target_snapshot(&snapshot);
        let key = snapshot.target_id.to_string();
        self.spawn(async move {
            if let Ok(db) = crate::platform::browser::indexed_db::open_database_with_store(
                &db_name, version, &store,
            )
            .await
            {
                let _ =
                    crate::platform::browser::indexed_db::put_string(&db, &store, &key, &payload)
                        .await;
            }
        });
    }

    fn clear_target_metadata(&self, target_id: i32) {
        let store = self.targets_store.clone();
        let db_name = self.db_name.clone();
        let version = self.version;
        let key = target_id.to_string();
        self.spawn(async move {
            if let Ok(db) = crate::platform::browser::indexed_db::open_database_with_store(
                &db_name, version, &store,
            )
            .await
            {
                let _ = crate::platform::browser::indexed_db::delete_key(&db, &store, &key).await;
            }
        });
    }

    fn save_document_overlay(&self, key: &DocumentKey, overlay: &[WriteOperation]) {
        let store = self.overlays_store.clone();
        let db_name = self.db_name.clone();
        let version = self.version;
        let key_path = key.path().canonical_string();
        let payload = Self::encode_overlay(key, overlay);
        self.spawn(async move {
            if let Ok(db) = crate::platform::browser::indexed_db::open_database_with_store(
                &db_name, version, &store,
            )
            .await
            {
                let _ = crate::platform::browser::indexed_db::put_string(
                    &db, &store, &key_path, &payload,
                )
                .await;
            }
        });
    }

    fn clear_document_overlay(&self, key: &DocumentKey) {
        let store = self.overlays_store.clone();
        let db_name = self.db_name.clone();
        let version = self.version;
        let key_path = key.path().canonical_string();
        self.spawn(async move {
            if let Ok(db) = crate::platform::browser::indexed_db::open_database_with_store(
                &db_name, version, &store,
            )
            .await
            {
                let _ =
                    crate::platform::browser::indexed_db::delete_key(&db, &store, &key_path).await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
    use base64::Engine;
    use serde_json::json;

    use crate::firestore::model::{DatabaseId, ResourcePath};
    use crate::firestore::remote::network::NetworkLayer;
    use crate::firestore::remote::remote_store::RemoteStore;
    use crate::firestore::remote::streams::ListenTarget;
    use crate::firestore::remote::syncer_bridge::RemoteSyncerBridge;
    use crate::firestore::remote::RemoteSyncer;
    use crate::firestore::remote::{
        InMemoryTransport, MultiplexedConnection, StreamingDatastoreImpl,
    };
    use crate::firestore::remote::{JsonProtoSerializer, NoopTokenProvider, TokenProviderArc};
    use crate::platform::runtime;

    fn query_definition() -> crate::firestore::QueryDefinition {
        crate::firestore::QueryDefinition {
            collection_path: ResourcePath::from_string("cities").unwrap(),
            parent_path: ResourcePath::root(),
            collection_id: "cities".to_string(),
            collection_group: None,
            filters: Vec::new(),
            request_order_by: Vec::new(),
            result_order_by: Vec::new(),
            limit: None,
            limit_type: crate::firestore::LimitType::First,
            request_start_at: None,
            request_end_at: None,
            result_start_at: None,
            result_end_at: None,
            projection: None,
        }
    }

    fn delete_operation_for(key: &DocumentKey) -> WriteOperation {
        WriteOperation::Delete { key: key.clone() }
    }

    #[tokio::test]
    async fn integrates_with_remote_store() {
        let (client_transport, server_transport) = InMemoryTransport::pair();
        let client_connection = Arc::new(MultiplexedConnection::new(client_transport));
        let server_connection = Arc::new(MultiplexedConnection::new(server_transport));

        let datastore = StreamingDatastoreImpl::new(Arc::clone(&client_connection));
        let datastore: Arc<dyn crate::firestore::remote::StreamingDatastore> = Arc::new(datastore);
        let token_provider: TokenProviderArc = Arc::new(NoopTokenProvider::default());
        let network = NetworkLayer::builder(datastore, token_provider).build();
        let serializer = JsonProtoSerializer::new(DatabaseId::new("test", "(default)"));

        let local_store = Arc::new(MemoryLocalStore::new());
        let bridge = Arc::new(RemoteSyncerBridge::new(Arc::clone(&local_store)));
        let remote_store = RemoteStore::new(
            network,
            serializer.clone(),
            bridge.clone() as Arc<dyn RemoteSyncer>,
        );

        remote_store.enable_network().await.expect("enable network");

        let target = ListenTarget::for_query(&serializer, 1, &query_definition()).unwrap();
        remote_store.listen(target).await.expect("listen target");

        let listen_stream = server_connection
            .open_stream()
            .await
            .expect("listen stream");

        let add_target = listen_stream
            .next()
            .await
            .expect("addTarget frame")
            .expect("payload");
        let add_json: serde_json::Value = serde_json::from_slice(&add_target).unwrap();
        assert!(add_json.get("addTarget").is_some());

        let change = json!({
            "documentChange": {
                "document": {
                    "name": "projects/test/databases/(default)/documents/cities/sf",
                    "fields": {}
                },
                "targetIds": [1],
                "removedTargetIds": []
            }
        });
        listen_stream
            .send(serde_json::to_vec(&change).unwrap())
            .await
            .expect("send change");

        runtime::sleep(Duration::from_millis(50)).await;
        let key = DocumentKey::from_string("cities/sf").unwrap();
        assert!(local_store.document(&key).await.is_some());

        let metadata = local_store.target_metadata_snapshot(1).unwrap();
        assert!(metadata.remote_keys.contains(&key));

        let batch_id = local_store
            .queue_mutation_batch(&bridge, vec![delete_operation_for(&key)])
            .await
            .expect("queue batch");
        remote_store.pump_writes().await.expect("pump writes");

        let write_stream = server_connection.open_stream().await.expect("write stream");

        let handshake = write_stream
            .next()
            .await
            .expect("handshake frame")
            .expect("payload");
        let handshake_json: serde_json::Value = serde_json::from_slice(&handshake).unwrap();
        assert_eq!(
            handshake_json.get("database"),
            Some(&json!("projects/test/databases/(default)"))
        );

        let handshake_response = json!({
            "streamToken": BASE64_STANDARD.encode([1u8, 2, 3]),
            "writeResults": []
        });
        write_stream
            .send(serde_json::to_vec(&handshake_response).unwrap())
            .await
            .expect("send handshake response");

        let write_request = write_stream
            .next()
            .await
            .expect("write frame")
            .expect("payload");
        let write_json: serde_json::Value = serde_json::from_slice(&write_request).unwrap();
        let writes = write_json
            .get("writes")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap();
        assert_eq!(writes.len(), 1);

        let commit_response = json!({
            "streamToken": BASE64_STANDARD.encode([9u8, 8, 7]),
            "commitTime": "2020-01-01T00:00:00Z",
            "writeResults": [
                {
                    "updateTime": "2020-01-01T00:00:00Z",
                    "transformResults": []
                }
            ]
        });
        write_stream
            .send(serde_json::to_vec(&commit_response).unwrap())
            .await
            .expect("send write response");

        runtime::sleep(Duration::from_millis(50)).await;

        assert_eq!(local_store.successful_batch_ids().await, vec![batch_id]);
        assert!(local_store.failed_batch_ids().await.is_empty());
        assert!(local_store.outstanding_batch_ids().await.is_empty());
        assert_eq!(local_store.last_stream_token(), Some(vec![9, 8, 7]));
        let recorded = local_store.recorded_write_results();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, batch_id);
        assert!(local_store.overlays_snapshot().await.is_empty());
    }
}
