use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Debug, Formatter};
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use async_lock::Mutex;
use async_trait::async_trait;

#[cfg(all(
    feature = "wasm-web",
    feature = "experimental-indexed-db",
    target_arch = "wasm32"
))]
use crate::firestore::model::DatabaseId;
#[cfg(all(
    feature = "wasm-web",
    feature = "experimental-indexed-db",
    target_arch = "wasm32"
))]
use crate::firestore::remote::serializer::JsonProtoSerializer;
#[cfg(all(
    feature = "wasm-web",
    feature = "experimental-indexed-db",
    target_arch = "wasm32"
))]
use base64::Engine;
#[cfg(all(
    feature = "wasm-web",
    feature = "experimental-indexed-db",
    target_arch = "wasm32"
))]
use serde_json::{json, Value};

use crate::firestore::api::query::compute_doc_changes;
use crate::firestore::api::{
    query::Query, query::QuerySnapshot, query::QuerySnapshotMetadata, snapshot::DocumentSnapshot,
    snapshot::SnapshotMetadata,
};
use crate::firestore::error::{invalid_argument, FirestoreError, FirestoreResult};
use crate::firestore::local::overlay::apply_document_overlays;
use crate::firestore::model::{DocumentKey, Timestamp};
use crate::firestore::query_evaluator::apply_query_to_documents;
use crate::firestore::remote::datastore::WriteOperation;
use crate::firestore::remote::mutation::{MutationBatch, MutationBatchResult};
use crate::firestore::remote::remote_event::RemoteEvent;
use crate::firestore::remote::streams::write::WriteResult;
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
    pub fn new(target_id: i32) -> Self {
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
    fn save_query_view_state(&self, _target_id: i32, _state: &PersistedQueryViewState) {}
    fn clear_query_view_state(&self, _target_id: i32) {}
    fn schedule_initial_load(&self, _store: Arc<MemoryLocalStore>) {}
}

type QueryListenerCallback = Arc<dyn Fn(QuerySnapshot) + Send + Sync>;

#[derive(Clone)]
struct QueryListenerEntry {
    id: u64,
    query: Query,
    callback: QueryListenerCallback,
    last_metadata: Option<QuerySnapshotMetadata>,
    last_documents: Vec<DocumentSnapshot>,
}

#[derive(Clone)]
pub struct PersistedQueryViewState {
    metadata: QuerySnapshotMetadata,
    documents: Vec<DocumentSnapshot>,
}

impl PersistedQueryViewState {
    fn new(mut metadata: QuerySnapshotMetadata, documents: Vec<DocumentSnapshot>) -> Self {
        metadata.set_sync_state_changed(false);
        Self {
            metadata,
            documents,
        }
    }

    fn metadata(&self) -> &QuerySnapshotMetadata {
        &self.metadata
    }

    #[allow(dead_code)]
    fn documents(&self) -> &[DocumentSnapshot] {
        &self.documents
    }

    fn clone_documents(&self) -> Vec<DocumentSnapshot> {
        self.documents.clone()
    }
}

struct QueryViewState {
    documents: Vec<DocumentSnapshot>,
    has_pending_writes: bool,
    resume_token: Option<Vec<u8>>,
    snapshot_version: Option<Timestamp>,
    from_cache: bool,
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
    query_listeners: StdMutex<BTreeMap<i32, Vec<QueryListenerEntry>>>,
    listener_counter: AtomicU64,
    restored_query_views: StdMutex<BTreeMap<i32, PersistedQueryViewState>>,
}

impl Debug for MemoryLocalStore {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryLocalStore").finish()
    }
}

impl MemoryLocalStore {
    fn new_internal(persistence: Option<Arc<dyn LocalStorePersistence>>) -> Self {
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
            persistence,
            query_listeners: StdMutex::new(BTreeMap::new()),
            listener_counter: AtomicU64::new(1),
            restored_query_views: StdMutex::new(BTreeMap::new()),
        }
    }

    pub fn new() -> Self {
        Self::new_internal(None)
    }

    pub fn with_persistence(persistence: Arc<dyn LocalStorePersistence>) -> Arc<Self> {
        let store =
            Arc::new(Self::new_internal(Some(Arc::clone(&persistence)))) as Arc<MemoryLocalStore>;
        persistence.schedule_initial_load(Arc::clone(&store));
        store
    }

    #[cfg(all(
        feature = "wasm-web",
        feature = "experimental-indexed-db",
        target_arch = "wasm32"
    ))]
    pub fn new_with_indexed_db(db_name: impl Into<String>) -> Arc<Self> {
        let persistence = Arc::new(IndexedDbPersistence::new(db_name));
        Self::with_persistence(persistence)
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

        self.emit_all_query_snapshots().await?;

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

    pub async fn overlay_keys(&self) -> BTreeSet<DocumentKey> {
        self.overlays.lock().await.keys().cloned().collect()
    }

    pub fn track_limbo_document(&self, key: DocumentKey) {
        self.limbo_documents.lock().unwrap().insert(key);
    }

    pub fn target_metadata_map(&self) -> BTreeMap<i32, TargetMetadataSnapshot> {
        self.target_metadata.lock().unwrap().clone()
    }

    pub fn restore_target_snapshot(&self, snapshot: TargetMetadataSnapshot) {
        self.target_metadata
            .lock()
            .unwrap()
            .insert(snapshot.target_id, snapshot);
    }

    pub fn restore_query_view_state(&self, target_id: i32, state: PersistedQueryViewState) {
        self.restored_query_views
            .lock()
            .unwrap()
            .insert(target_id, state);
    }

    fn stored_query_view_state(&self, target_id: i32) -> Option<PersistedQueryViewState> {
        self.restored_query_views
            .lock()
            .unwrap()
            .get(&target_id)
            .cloned()
    }

    fn record_query_view_state(
        &self,
        target_id: i32,
        metadata: QuerySnapshotMetadata,
        documents: Vec<DocumentSnapshot>,
    ) {
        let state = PersistedQueryViewState::new(metadata, documents);
        {
            let mut guard = self.restored_query_views.lock().unwrap();
            guard.insert(target_id, state.clone());
        }
        if let Some(persistence) = &self.persistence {
            persistence.save_query_view_state(target_id, &state);
        }
    }

    fn clear_persisted_query_view_state(&self, target_id: i32) {
        {
            let mut guard = self.restored_query_views.lock().unwrap();
            guard.remove(&target_id);
        }
        if let Some(persistence) = &self.persistence {
            persistence.clear_query_view_state(target_id);
        }
    }

    fn dispatch_restored_snapshot(
        &self,
        target_id: i32,
        listener_id: u64,
        query: Query,
        state: PersistedQueryViewState,
    ) {
        let callback = {
            let guard = self.query_listeners.lock().unwrap();
            guard.get(&target_id).and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| entry.id == listener_id)
                    .map(|entry| Arc::clone(&entry.callback))
            })
        };

        if let Some(callback) = callback {
            let snapshot = QuerySnapshot::new(
                query,
                state.clone_documents(),
                state.metadata().clone(),
                Vec::new(),
            );
            (callback)(snapshot);
        }
    }

    pub async fn restore_overlay_key(&self, key: DocumentKey) {
        self.overlays
            .lock()
            .await
            .entry(key)
            .or_insert_with(Vec::new);
    }

    async fn document_snapshot_for_key(
        &self,
        key: &DocumentKey,
        from_cache: bool,
    ) -> FirestoreResult<DocumentSnapshot> {
        let maybe_doc = {
            let guard = self.documents.lock().await;
            guard.get(key).cloned().flatten()
        };

        let overlay_ops = {
            let guard = self.overlays.lock().await;
            guard.get(key).cloned()
        };

        let has_overlay = overlay_ops
            .as_ref()
            .map(|ops| !ops.is_empty())
            .unwrap_or(false);

        let mut data = maybe_doc.map(|doc| doc.fields.clone());
        if let Some(ops) = overlay_ops.as_ref() {
            if !ops.is_empty() {
                data = apply_document_overlays(data, ops)?;
            }
        }

        let metadata = SnapshotMetadata::new(from_cache, has_overlay);
        Ok(DocumentSnapshot::new(key.clone(), data, metadata))
    }

    async fn compute_query_state(
        &self,
        target_id: i32,
        query: &Query,
    ) -> FirestoreResult<QueryViewState> {
        let target_snapshot = self.target_metadata_snapshot(target_id);
        let from_cache = target_snapshot
            .as_ref()
            .map(|snapshot| !snapshot.current)
            .unwrap_or(true);
        let resume_token = target_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.resume_token.clone());
        let snapshot_version = target_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.snapshot_version);

        let definition = query.definition();

        let mut keys = BTreeSet::new();
        if let Some(snapshot) = target_snapshot.as_ref() {
            for key in &snapshot.remote_keys {
                if definition.matches_collection(key) {
                    keys.insert(key.clone());
                }
            }
        }

        for overlay_key in self.overlay_keys().await {
            if definition.matches_collection(&overlay_key) {
                keys.insert(overlay_key);
            }
        }

        let mut docs = Vec::new();
        for key in keys {
            docs.push(self.document_snapshot_for_key(&key, from_cache).await?);
        }

        let documents = apply_query_to_documents(docs, &definition);
        let has_pending_writes = documents.iter().any(|doc| doc.has_pending_writes());

        Ok(QueryViewState {
            documents,
            has_pending_writes,
            resume_token,
            snapshot_version,
            from_cache,
        })
    }

    async fn build_query_snapshot(
        &self,
        target_id: i32,
        query: &Query,
        previous_metadata: Option<&QuerySnapshotMetadata>,
        previous_documents: Option<&[DocumentSnapshot]>,
    ) -> FirestoreResult<QuerySnapshot> {
        let state = self.compute_query_state(target_id, query).await?;
        let previous_documents =
            previous_documents.and_then(|docs| if docs.is_empty() { None } else { Some(docs) });
        let doc_changes = compute_doc_changes(previous_documents, &state.documents);

        let mut metadata = QuerySnapshotMetadata::new(
            state.from_cache,
            state.has_pending_writes,
            false,
            state.resume_token.clone(),
            state.snapshot_version,
        );

        if let Some(previous) = previous_metadata {
            let sync_changed = previous.from_cache() != metadata.from_cache()
                || previous.has_pending_writes() != metadata.has_pending_writes();
            metadata.set_sync_state_changed(sync_changed);
        } else {
            metadata.set_sync_state_changed(true);
        }

        Ok(QuerySnapshot::new(
            query.clone(),
            state.documents,
            metadata,
            doc_changes,
        ))
    }

    async fn emit_query_snapshot(&self, target_id: i32) -> FirestoreResult<()> {
        let listeners = {
            let guard = self.query_listeners.lock().unwrap();
            guard.get(&target_id).cloned()
        };

        if let Some(entries) = listeners {
            let mut updates: Vec<(u64, QuerySnapshotMetadata, Vec<DocumentSnapshot>)> = Vec::new();
            let mut callbacks = Vec::new();
            let mut state_for_persistence: Option<(QuerySnapshotMetadata, Vec<DocumentSnapshot>)> =
                None;

            for entry in entries {
                let previous_docs = if entry.last_documents.is_empty() {
                    None
                } else {
                    Some(entry.last_documents.as_slice())
                };
                let snapshot = self
                    .build_query_snapshot(
                        target_id,
                        &entry.query,
                        entry.last_metadata.as_ref(),
                        previous_docs,
                    )
                    .await?;
                let metadata = snapshot.metadata().clone();
                let documents = snapshot.documents().to_vec();
                if state_for_persistence.is_none() {
                    state_for_persistence = Some((metadata.clone(), documents.clone()));
                }
                updates.push((entry.id, metadata, documents));
                callbacks.push((Arc::clone(&entry.callback), snapshot));
            }

            {
                let mut guard = self.query_listeners.lock().unwrap();
                if let Some(entries_mut) = guard.get_mut(&target_id) {
                    for (id, metadata, documents) in updates {
                        if let Some(entry) = entries_mut.iter_mut().find(|entry| entry.id == id) {
                            entry.last_metadata = Some(metadata);
                            entry.last_documents = documents;
                        }
                    }
                }
            }

            if let Some((metadata, documents)) = state_for_persistence {
                self.record_query_view_state(target_id, metadata, documents);
            }

            for (callback, snapshot) in callbacks {
                (callback)(snapshot);
            }
        }
        Ok(())
    }

    async fn emit_all_query_snapshots(&self) -> FirestoreResult<()> {
        let targets: Vec<i32> = {
            let guard = self.query_listeners.lock().unwrap();
            guard.keys().cloned().collect()
        };
        for target_id in targets {
            self.emit_query_snapshot(target_id).await?;
        }
        Ok(())
    }

    async fn notify_query_listeners_for_event(&self, event: &RemoteEvent) -> FirestoreResult<()> {
        let mut target_ids: BTreeSet<i32> = event.target_changes.keys().cloned().collect();
        target_ids.extend(event.target_resets.iter().cloned());
        for target_id in target_ids {
            self.emit_query_snapshot(target_id).await?;
        }
        Ok(())
    }

    async fn register_query_listener_internal(
        self: &Arc<Self>,
        target_id: i32,
        query: Query,
        callback: QueryListenerCallback,
    ) -> FirestoreResult<QueryListenerRegistration> {
        let id = self.listener_counter.fetch_add(1, Ordering::SeqCst);
        let mut seed_metadata = None;
        let mut seed_documents = Vec::new();
        let mut restored_snapshot = None;
        if let Some(state) = self.stored_query_view_state(target_id) {
            seed_metadata = Some(state.metadata().clone());
            seed_documents = state.clone_documents();
            restored_snapshot = Some((query.clone(), state));
        } else {
            let should_seed = self
                .target_metadata_snapshot(target_id)
                .map(|snapshot| snapshot.current && snapshot.resume_token.is_some())
                .unwrap_or(false);

            if should_seed {
                let state = self.compute_query_state(target_id, &query).await?;
                let metadata = QuerySnapshotMetadata::new(
                    state.from_cache,
                    state.has_pending_writes,
                    false,
                    state.resume_token.clone(),
                    state.snapshot_version,
                );
                seed_metadata = Some(metadata);
                seed_documents = state.documents;
            }
        }

        {
            let mut guard = self.query_listeners.lock().unwrap();
            guard
                .entry(target_id)
                .or_insert_with(Vec::new)
                .push(QueryListenerEntry {
                    id,
                    query: query.clone(),
                    callback,
                    last_metadata: seed_metadata.clone(),
                    last_documents: seed_documents.clone(),
                });
        }

        let registration = QueryListenerRegistration::new(Arc::clone(self), target_id, id);

        if let Some((query, state)) = restored_snapshot {
            self.dispatch_restored_snapshot(target_id, id, query, state);
            return Ok(registration);
        }

        self.emit_query_snapshot(target_id).await?;
        Ok(registration)
    }

    fn remove_query_listener(&self, target_id: i32, listener_id: u64) {
        let mut guard = self.query_listeners.lock().unwrap();
        if let Some(entries) = guard.get_mut(&target_id) {
            entries.retain(|entry| entry.id != listener_id);
            if entries.is_empty() {
                guard.remove(&target_id);
                self.clear_persisted_query_view_state(target_id);
            }
        }
    }

    pub fn synchronize_remote_keys(&self, bridge: &RemoteSyncerBridge<MemoryLocalStore>) {
        let snapshots = self.target_metadata.lock().unwrap().clone();
        for (target_id, snapshot) in snapshots {
            if !snapshot.remote_keys.is_empty() {
                bridge.seed_remote_keys(target_id, snapshot.remote_keys.clone());
            }
        }
    }

    pub async fn register_query_listener(
        self: &Arc<Self>,
        target_id: i32,
        query: Query,
        callback: QueryListenerCallback,
    ) -> FirestoreResult<QueryListenerRegistration> {
        self.register_query_listener_internal(target_id, query, callback)
            .await
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

        let cleared_views = {
            let mut views = self.restored_query_views.lock().unwrap();
            let ids = views.keys().copied().collect::<Vec<_>>();
            views.clear();
            ids
        };

        if let Some(persistence) = &self.persistence {
            for target_id in cleared_views {
                persistence.clear_query_view_state(target_id);
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

pub struct QueryListenerRegistration {
    store: Arc<MemoryLocalStore>,
    target_id: i32,
    listener_id: u64,
    detached: bool,
}

impl QueryListenerRegistration {
    fn new(store: Arc<MemoryLocalStore>, target_id: i32, listener_id: u64) -> Self {
        Self {
            store,
            target_id,
            listener_id,
            detached: false,
        }
    }

    pub fn detach(&mut self) {
        if !self.detached {
            self.store
                .remove_query_listener(self.target_id, self.listener_id);
            self.detached = true;
        }
    }
}

impl Drop for QueryListenerRegistration {
    fn drop(&mut self) {
        if !self.detached {
            self.store
                .remove_query_listener(self.target_id, self.listener_id);
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

        if !event.document_updates.is_empty() || !event.target_changes.is_empty() {
            let mut limbo = self.limbo_documents.lock().unwrap();
            for key in event.document_updates.keys() {
                limbo.remove(key);
            }
            for change in event.target_changes.values() {
                for key in change
                    .added_documents
                    .iter()
                    .chain(change.modified_documents.iter())
                    .chain(change.removed_documents.iter())
                {
                    limbo.remove(key);
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

        self.notify_query_listeners_for_event(&event).await?;

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
        self.emit_all_query_snapshots().await?;
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
        self.emit_all_query_snapshots().await?;
        Ok(())
    }

    async fn handle_credential_change(&self) -> FirestoreResult<()> {
        self.clear_all().await;
        self.emit_all_query_snapshots().await?;
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
        let (prior_remote_keys, snapshot, other_remote_keys) = {
            let mut targets = self.target_metadata.lock().unwrap();
            let (prior_remote_keys, snapshot) = {
                let entry = targets
                    .entry(target_id)
                    .or_insert_with(|| TargetMetadataSnapshot::new(target_id));
                let prior = entry.remote_keys.clone();
                entry.remote_keys.clear();
                entry.resume_token = None;
                entry.snapshot_version = None;
                entry.current = false;
                (prior, entry.clone())
            };
            let other_remote_keys = targets
                .iter()
                .filter_map(|(&id, meta)| {
                    if id == target_id {
                        None
                    } else {
                        Some(meta.remote_keys.clone())
                    }
                })
                .fold(BTreeSet::new(), |mut acc, keys| {
                    acc.extend(keys);
                    acc
                });
            (prior_remote_keys, snapshot, other_remote_keys)
        };

        if let Some(persistence) = persistence {
            persistence.clear_target_metadata(target_id);
            persistence.save_target_metadata(snapshot);
        }

        if !prior_remote_keys.is_empty() {
            let mut limbo = self.limbo_documents.lock().unwrap();
            for key in prior_remote_keys {
                if !other_remote_keys.contains(&key) {
                    limbo.insert(key);
                }
            }
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
    view_states_store: String,
    version: u32,
}

#[cfg(all(
    feature = "wasm-web",
    feature = "experimental-indexed-db",
    target_arch = "wasm32"
))]
const TARGETS_CATALOG_KEY: &str = "__targets_catalog__";
#[cfg(all(
    feature = "wasm-web",
    feature = "experimental-indexed-db",
    target_arch = "wasm32"
))]
const OVERLAYS_CATALOG_KEY: &str = "__overlays_catalog__";
#[cfg(all(
    feature = "wasm-web",
    feature = "experimental-indexed-db",
    target_arch = "wasm32"
))]
const VIEW_STATES_CATALOG_KEY: &str = "__view_states_catalog__";

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
            view_states_store: "firestore_view_states".into(),
            version: 2,
        }
    }

    fn spawn<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + 'static,
    {
        wasm_bindgen_futures::spawn_local(future);
    }

    fn serializer() -> JsonProtoSerializer {
        JsonProtoSerializer::new(DatabaseId::default("offline"))
    }

    fn encode_view_metadata(metadata: &QuerySnapshotMetadata) -> Value {
        let mut object = serde_json::Map::new();
        object.insert("fromCache".into(), json!(metadata.from_cache()));
        object.insert(
            "hasPendingWrites".into(),
            json!(metadata.has_pending_writes()),
        );
        if let Some(token) = metadata.resume_token() {
            object.insert(
                "resumeToken".into(),
                json!(base64::engine::general_purpose::STANDARD.encode(token)),
            );
        }
        if let Some(version) = metadata.snapshot_version() {
            object.insert(
                "snapshotVersion".into(),
                json!({
                    "seconds": version.seconds,
                    "nanos": version.nanos,
                }),
            );
        }
        Value::Object(object)
    }

    fn encode_document(serializer: &JsonProtoSerializer, snapshot: &DocumentSnapshot) -> Value {
        let mut object = serde_json::Map::new();
        object.insert(
            "key".into(),
            json!(snapshot.document_key().path().canonical_string()),
        );
        object.insert("fromCache".into(), json!(snapshot.from_cache()));
        object.insert(
            "hasPendingWrites".into(),
            json!(snapshot.has_pending_writes()),
        );
        if let Some(map) = snapshot.map_value() {
            object.insert("fields".into(), serializer.encode_document_fields(map));
        }
        Value::Object(object)
    }

    fn encode_view_state(state: &PersistedQueryViewState) -> String {
        let serializer = Self::serializer();
        let documents = state
            .documents()
            .iter()
            .map(|doc| Self::encode_document(&serializer, doc))
            .collect::<Vec<_>>();
        json!({
            "metadata": Self::encode_view_metadata(state.metadata()),
            "documents": documents,
        })
        .to_string()
    }

    fn decode_view_metadata(value: &Value) -> Option<QuerySnapshotMetadata> {
        let object = value.as_object()?;
        let from_cache = object
            .get("fromCache")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let has_pending = object
            .get("hasPendingWrites")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let resume_token = object
            .get("resumeToken")
            .and_then(Value::as_str)
            .and_then(|token| base64::engine::general_purpose::STANDARD.decode(token).ok());
        let snapshot_version = object.get("snapshotVersion").and_then(|json| {
            let seconds = json.get("seconds")?.as_i64()?;
            let nanos = json.get("nanos")?.as_i64()? as i32;
            Some(Timestamp::new(seconds, nanos))
        });
        Some(QuerySnapshotMetadata::new(
            from_cache,
            has_pending,
            false,
            resume_token,
            snapshot_version,
        ))
    }

    fn decode_document(
        serializer: &JsonProtoSerializer,
        value: &Value,
    ) -> Option<DocumentSnapshot> {
        let object = value.as_object()?;
        let key_str = object.get("key")?.as_str()?;
        let key = DocumentKey::from_string(key_str).ok()?;
        let from_cache = object
            .get("fromCache")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let has_pending = object
            .get("hasPendingWrites")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let data = if let Some(fields) = object.get("fields") {
            serializer.decode_document_fields(fields).ok().flatten()
        } else {
            None
        };
        let metadata = SnapshotMetadata::new(from_cache, has_pending);
        Some(DocumentSnapshot::new(key, data, metadata))
    }

    fn decode_view_state(payload: &str) -> Option<PersistedQueryViewState> {
        let value: Value = serde_json::from_str(payload).ok()?;
        let metadata_value = value.get("metadata")?;
        let documents_value = value.get("documents")?.as_array()?;
        let metadata = Self::decode_view_metadata(metadata_value)?;
        let serializer = Self::serializer();
        let mut documents = Vec::with_capacity(documents_value.len());
        for entry in documents_value {
            let doc = Self::decode_document(&serializer, entry)?;
            documents.push(doc);
        }
        Some(PersistedQueryViewState::new(metadata, documents))
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
            json!({
                "seconds": ts.seconds,
                "nanos": ts.nanos,
            })
        });

        json!({
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
        json!({
            "key": key.path().canonical_string(),
            "writes": write_paths,
        })
        .to_string()
    }

    async fn get_catalog(
        db: &web_sys::IdbDatabase,
        store: &str,
        catalog_key: &str,
    ) -> crate::platform::browser::indexed_db::IndexedDbResult<BTreeSet<String>> {
        let existing =
            crate::platform::browser::indexed_db::get_string(db, store, catalog_key).await?;
        if let Some(json) = existing {
            let parsed: Value = serde_json::from_str(&json).unwrap_or_else(|_| json!([]));
            if let Some(array) = parsed.as_array() {
                let entries = array
                    .iter()
                    .filter_map(|value| value.as_str().map(|s| s.to_string()))
                    .collect();
                Ok(entries)
            } else {
                Ok(BTreeSet::new())
            }
        } else {
            Ok(BTreeSet::new())
        }
    }

    async fn save_catalog(
        db: &web_sys::IdbDatabase,
        store: &str,
        catalog_key: &str,
        entries: &BTreeSet<String>,
    ) -> crate::platform::browser::indexed_db::IndexedDbResult<()> {
        let payload = json!(entries.iter().collect::<Vec<_>>()).to_string();
        crate::platform::browser::indexed_db::put_string(db, store, catalog_key, &payload).await
    }

    fn decode_target_snapshot(payload: &str) -> Option<TargetMetadataSnapshot> {
        let value: Value = serde_json::from_str(payload).ok()?;
        let target_id = value.get("targetId")?.as_i64()? as i32;
        let resume_token = value
            .get("resumeToken")
            .and_then(Value::as_str)
            .and_then(|token| base64::engine::general_purpose::STANDARD.decode(token).ok());
        let snapshot_version = value.get("snapshotVersion").and_then(|json| {
            let seconds = json.get("seconds")?.as_i64()?;
            let nanos = json.get("nanos")?.as_i64()? as i32;
            Some(Timestamp::new(seconds, nanos))
        });
        let current = value
            .get("current")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let remote_keys = value
            .get("remoteKeys")
            .and_then(Value::as_array)
            .map(|array| {
                array
                    .iter()
                    .filter_map(|entry| {
                        entry
                            .as_str()
                            .and_then(|path| DocumentKey::from_string(path).ok())
                    })
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();

        Some(TargetMetadataSnapshot {
            target_id,
            resume_token,
            snapshot_version,
            current,
            remote_keys,
        })
    }

    fn decode_overlay(payload: &str) -> Option<DocumentKey> {
        let value: Value = serde_json::from_str(payload).ok()?;
        let key_path = value.get("key")?.as_str()?;
        DocumentKey::from_string(key_path).ok()
    }

    fn schedule_initial_load_internal(&self, store: Arc<MemoryLocalStore>) {
        let db_name = self.db_name.clone();
        let targets_store = self.targets_store.clone();
        let overlays_store = self.overlays_store.clone();
        let view_states_store = self.view_states_store.clone();
        let version = self.version;

        self.spawn(async move {
            if let Ok(db) = crate::platform::browser::indexed_db::open_database_with_store(
                &db_name,
                version,
                &targets_store,
            )
            .await
            {
                if let Ok(catalog) =
                    Self::get_catalog(&db, &targets_store, TARGETS_CATALOG_KEY).await
                {
                    for target_key in catalog {
                        if let Ok(Some(payload)) = crate::platform::browser::indexed_db::get_string(
                            &db,
                            &targets_store,
                            &target_key,
                        )
                        .await
                        {
                            if let Some(snapshot) = Self::decode_target_snapshot(&payload) {
                                store.restore_target_snapshot(snapshot);
                            }
                        }
                    }
                }
            }

            if let Ok(db) = crate::platform::browser::indexed_db::open_database_with_store(
                &db_name,
                version,
                &overlays_store,
            )
            .await
            {
                if let Ok(catalog) =
                    Self::get_catalog(&db, &overlays_store, OVERLAYS_CATALOG_KEY).await
                {
                    for key_path in catalog {
                        if let Ok(Some(payload)) = crate::platform::browser::indexed_db::get_string(
                            &db,
                            &overlays_store,
                            &key_path,
                        )
                        .await
                        {
                            if let Some(key) = Self::decode_overlay(&payload) {
                                store.restore_overlay_key(key).await;
                            }
                        }
                    }
                }
            }

            if let Ok(db) = crate::platform::browser::indexed_db::open_database_with_store(
                &db_name,
                version,
                &view_states_store,
            )
            .await
            {
                if let Ok(catalog) =
                    Self::get_catalog(&db, &view_states_store, VIEW_STATES_CATALOG_KEY).await
                {
                    for key in catalog {
                        if let Ok(Some(payload)) = crate::platform::browser::indexed_db::get_string(
                            &db,
                            &view_states_store,
                            &key,
                        )
                        .await
                        {
                            if let Ok(target_id) = key.parse::<i32>() {
                                if let Some(state) = Self::decode_view_state(&payload) {
                                    store.restore_query_view_state(target_id, state);
                                }
                            }
                        }
                    }
                }
            }
        });
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
                if let Ok(mut catalog) = Self::get_catalog(&db, &store, TARGETS_CATALOG_KEY).await {
                    if catalog.insert(key.clone()) {
                        let _ =
                            Self::save_catalog(&db, &store, TARGETS_CATALOG_KEY, &catalog).await;
                    }
                }
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
                if let Ok(mut catalog) = Self::get_catalog(&db, &store, TARGETS_CATALOG_KEY).await {
                    if catalog.remove(&key) {
                        let _ =
                            Self::save_catalog(&db, &store, TARGETS_CATALOG_KEY, &catalog).await;
                    }
                }
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
                if let Ok(mut catalog) = Self::get_catalog(&db, &store, OVERLAYS_CATALOG_KEY).await
                {
                    if catalog.insert(key_path.clone()) {
                        let _ =
                            Self::save_catalog(&db, &store, OVERLAYS_CATALOG_KEY, &catalog).await;
                    }
                }
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
                if let Ok(mut catalog) = Self::get_catalog(&db, &store, OVERLAYS_CATALOG_KEY).await
                {
                    if catalog.remove(&key_path) {
                        let _ =
                            Self::save_catalog(&db, &store, OVERLAYS_CATALOG_KEY, &catalog).await;
                    }
                }
            }
        });
    }

    fn save_query_view_state(&self, target_id: i32, state: &PersistedQueryViewState) {
        let store = self.view_states_store.clone();
        let db_name = self.db_name.clone();
        let version = self.version;
        let key = target_id.to_string();
        let payload = Self::encode_view_state(state);
        self.spawn(async move {
            if let Ok(db) = crate::platform::browser::indexed_db::open_database_with_store(
                &db_name, version, &store,
            )
            .await
            {
                let _ =
                    crate::platform::browser::indexed_db::put_string(&db, &store, &key, &payload)
                        .await;
                if let Ok(mut catalog) =
                    Self::get_catalog(&db, &store, VIEW_STATES_CATALOG_KEY).await
                {
                    if catalog.insert(key.clone()) {
                        let _ = Self::save_catalog(&db, &store, VIEW_STATES_CATALOG_KEY, &catalog)
                            .await;
                    }
                }
            }
        });
    }

    fn clear_query_view_state(&self, target_id: i32) {
        let store = self.view_states_store.clone();
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
                if let Ok(mut catalog) =
                    Self::get_catalog(&db, &store, VIEW_STATES_CATALOG_KEY).await
                {
                    if catalog.remove(&key) {
                        let _ = Self::save_catalog(&db, &store, VIEW_STATES_CATALOG_KEY, &catalog)
                            .await;
                    }
                }
            }
        });
    }

    fn schedule_initial_load(&self, store: Arc<MemoryLocalStore>) {
        self.schedule_initial_load_internal(store);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
    use base64::Engine;
    use serde_json::json;

    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex as StdMutex;

    use crate::app::api::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use crate::firestore::api::database::{get_firestore, Firestore};
    use crate::firestore::api::query::{DocumentChangeType, Query};
    use crate::firestore::model::{DatabaseId, ResourcePath};
    use crate::firestore::remote::datastore::{
        NoopTokenProvider, StreamingDatastoreImpl, TokenProviderArc,
    };
    use crate::firestore::remote::network::NetworkLayer;
    use crate::firestore::remote::remote_event::RemoteEvent;
    use crate::firestore::remote::remote_store::RemoteStore;
    use crate::firestore::remote::remote_syncer::RemoteSyncer;
    use crate::firestore::remote::serializer::JsonProtoSerializer;
    use crate::firestore::remote::stream::{InMemoryTransport, MultiplexedConnection};
    use crate::firestore::remote::streams::listen::ListenTarget;
    use crate::firestore::remote::syncer_bridge::RemoteSyncerBridge;
    use crate::firestore::value::{FirestoreValue, MapValue};
    use crate::platform::runtime;

    #[derive(Default)]
    struct RecordingPersistence {
        target_metadata: StdMutex<BTreeMap<i32, TargetMetadataSnapshot>>,
        view_states: StdMutex<BTreeMap<i32, PersistedQueryViewState>>,
    }

    impl RecordingPersistence {
        fn new() -> Self {
            Self::default()
        }

        fn view_state_for(&self, target_id: i32) -> Option<PersistedQueryViewState> {
            self.view_states.lock().unwrap().get(&target_id).cloned()
        }
    }

    impl LocalStorePersistence for RecordingPersistence {
        fn save_target_metadata(&self, snapshot: TargetMetadataSnapshot) {
            self.target_metadata
                .lock()
                .unwrap()
                .insert(snapshot.target_id, snapshot);
        }

        fn clear_target_metadata(&self, target_id: i32) {
            self.target_metadata.lock().unwrap().remove(&target_id);
        }

        fn save_document_overlay(&self, _: &DocumentKey, _: &[WriteOperation]) {}

        fn clear_document_overlay(&self, _: &DocumentKey) {}

        fn save_query_view_state(&self, target_id: i32, state: &PersistedQueryViewState) {
            self.view_states
                .lock()
                .unwrap()
                .insert(target_id, state.clone());
        }

        fn clear_query_view_state(&self, target_id: i32) {
            self.view_states.lock().unwrap().remove(&target_id);
        }

        fn schedule_initial_load(&self, store: Arc<MemoryLocalStore>) {
            let targets = {
                let guard = self.target_metadata.lock().unwrap();
                guard.values().cloned().collect::<Vec<_>>()
            };
            for snapshot in targets {
                store.restore_target_snapshot(snapshot);
            }

            let view_states = {
                let guard = self.view_states.lock().unwrap();
                guard
                    .iter()
                    .map(|(target_id, state)| (*target_id, state.clone()))
                    .collect::<Vec<_>>()
            };
            for (target_id, state) in view_states {
                store.restore_query_view_state(target_id, state);
            }
        }
    }

    fn unique_app_settings() -> FirebaseAppSettings {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!(
                "firestore-memory-local-{}",
                COUNTER.fetch_add(1, Ordering::SeqCst)
            )),
            ..Default::default()
        }
    }

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
        let datastore: Arc<dyn crate::firestore::remote::datastore::StreamingDatastore> =
            Arc::new(datastore);
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

    #[tokio::test]
    async fn target_reset_marks_limbo_documents() {
        let store = Arc::new(MemoryLocalStore::new());
        let key = DocumentKey::from_string("cities/sf").unwrap();
        let mut snapshot = TargetMetadataSnapshot::new(1);
        snapshot.remote_keys.insert(key.clone());
        store.restore_target_snapshot(snapshot);

        let bridge = Arc::new(RemoteSyncerBridge::new(Arc::clone(&store)));
        store.synchronize_remote_keys(&bridge);

        let mut event = RemoteEvent::default();
        event.target_resets.insert(1);

        RemoteSyncer::apply_remote_event(&*bridge, event)
            .await
            .expect("apply event");

        let limbo = store.limbo_documents_snapshot();
        assert!(limbo.contains(&key));
    }

    #[tokio::test]
    async fn target_reset_skips_shared_documents() {
        let store = Arc::new(MemoryLocalStore::new());
        let key = DocumentKey::from_string("cities/sf").unwrap();

        let mut primary_snapshot = TargetMetadataSnapshot::new(1);
        primary_snapshot.remote_keys.insert(key.clone());
        store.restore_target_snapshot(primary_snapshot);

        let mut secondary_snapshot = TargetMetadataSnapshot::new(2);
        secondary_snapshot.remote_keys.insert(key.clone());
        store.restore_target_snapshot(secondary_snapshot);

        let bridge = Arc::new(RemoteSyncerBridge::new(Arc::clone(&store)));
        store.synchronize_remote_keys(&bridge);

        let mut event = RemoteEvent::default();
        event.target_resets.insert(1);
        RemoteSyncer::apply_remote_event(&*bridge, event)
            .await
            .expect("apply event");

        let limbo = store.limbo_documents_snapshot();
        assert!(limbo.is_empty());
    }

    #[tokio::test]
    async fn overlay_documents_survive_target_reset() {
        let store = Arc::new(MemoryLocalStore::new());
        let bridge = Arc::new(RemoteSyncerBridge::new(Arc::clone(&store)));
        store.synchronize_remote_keys(&bridge);

        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_app_settings()))
            .await
            .expect("initialize app");
        let firestore_arc = get_firestore(Some(app.clone())).await.expect("firestore");
        let firestore = Firestore::from_arc(firestore_arc);
        let path = ResourcePath::from_string("cities").expect("collection path");
        let query = Query::new(firestore, path).expect("query");

        let key = DocumentKey::from_string("cities/sf").unwrap();
        let mut fields = BTreeMap::new();
        fields.insert(
            "name".to_string(),
            FirestoreValue::from_string("San Francisco"),
        );
        let data = MapValue::new(fields);
        let write = WriteOperation::Set {
            key: key.clone(),
            data,
            mask: None,
            transforms: Vec::new(),
        };
        store
            .queue_mutation_batch(&bridge, vec![write])
            .await
            .expect("queue batch");

        let snapshots = Arc::new(StdMutex::new(Vec::new()));
        let callback_snapshots = Arc::clone(&snapshots);
        let registration = store
            .register_query_listener(
                1,
                query.clone(),
                Arc::new(move |snapshot| {
                    callback_snapshots.lock().unwrap().push(snapshot);
                }),
            )
            .await
            .expect("register listener");

        {
            let guard = snapshots.lock().unwrap();
            assert_eq!(guard.len(), 1);
            let snapshot = guard.last().unwrap();
            assert_eq!(snapshot.documents().len(), 1);
        }

        let mut event = RemoteEvent::default();
        event.target_resets.insert(1);
        RemoteSyncer::apply_remote_event(&*bridge, event)
            .await
            .expect("apply reset");

        {
            let guard = snapshots.lock().unwrap();
            assert!(guard.len() >= 2);
            let snapshot = guard.last().unwrap();
            assert_eq!(snapshot.documents().len(), 1);
            assert!(snapshot
                .doc_changes()
                .iter()
                .all(|change| change.change_type() != DocumentChangeType::Removed));
        }

        drop(registration);
    }

    #[tokio::test]
    async fn limbo_documents_cleared_on_follow_up_updates() {
        let store = Arc::new(MemoryLocalStore::new());
        let key = DocumentKey::from_string("cities/sf").unwrap();
        let mut snapshot = TargetMetadataSnapshot::new(1);
        snapshot.remote_keys.insert(key.clone());
        store.restore_target_snapshot(snapshot);

        let bridge = Arc::new(RemoteSyncerBridge::new(Arc::clone(&store)));
        store.synchronize_remote_keys(&bridge);

        let mut reset = RemoteEvent::default();
        reset.target_resets.insert(1);
        RemoteSyncer::apply_remote_event(&*bridge, reset)
            .await
            .expect("apply reset");

        assert!(store.limbo_documents_snapshot().contains(&key));

        let mut event = RemoteEvent::default();
        event.document_updates.insert(key.clone(), None);
        RemoteSyncer::apply_remote_event(&*bridge, event)
            .await
            .expect("apply update");

        assert!(!store.limbo_documents_snapshot().contains(&key));
    }

    #[tokio::test]
    async fn restores_view_state_from_persistence() {
        let raw_persistence = Arc::new(RecordingPersistence::new());
        let persistence: Arc<dyn LocalStorePersistence> = raw_persistence.clone();
        let store = MemoryLocalStore::with_persistence(persistence.clone());

        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_app_settings()))
            .await
            .expect("initialize app");
        let firestore_arc = get_firestore(Some(app.clone())).await.expect("firestore");
        let firestore = Firestore::from_arc(firestore_arc);
        let query = firestore.collection("cities").unwrap().query();

        let target_id = 1;
        let key = DocumentKey::from_string("cities/sf").unwrap();

        let mut metadata_snapshot = TargetMetadataSnapshot::new(target_id);
        metadata_snapshot.current = true;
        metadata_snapshot.resume_token = Some(vec![1, 2, 3]);
        metadata_snapshot.snapshot_version = Some(Timestamp::new(42, 0));
        metadata_snapshot.remote_keys.insert(key.clone());
        store.restore_target_snapshot(metadata_snapshot.clone());
        raw_persistence.save_target_metadata(metadata_snapshot);

        let mut fields = BTreeMap::new();
        fields.insert(
            "name".to_string(),
            FirestoreValue::from_string("San Francisco"),
        );
        let map = MapValue::new(fields);
        let watch_doc = WatchDocument {
            key: key.clone(),
            fields: map.clone(),
            update_time: Some(Timestamp::new(42, 0)),
            create_time: Some(Timestamp::new(40, 0)),
        };
        {
            let mut docs = store.documents.lock().await;
            docs.insert(key.clone(), Some(watch_doc.clone()));
        }

        let snapshots = Arc::new(StdMutex::new(Vec::new()));
        let callback_snapshots = Arc::clone(&snapshots);
        let registration = store
            .register_query_listener(
                target_id,
                query.clone(),
                Arc::new(move |snapshot| {
                    callback_snapshots.lock().unwrap().push(snapshot);
                }),
            )
            .await
            .expect("register listener");

        {
            let guard = snapshots.lock().unwrap();
            assert_eq!(guard.len(), 1);
        }
        assert!(raw_persistence.view_state_for(target_id).is_some());

        let store_reloaded = MemoryLocalStore::with_persistence(persistence.clone());
        {
            let mut docs = store_reloaded.documents.lock().await;
            docs.insert(key.clone(), Some(watch_doc.clone()));
        }

        let resumed_snapshots = Arc::new(StdMutex::new(Vec::new()));
        let callback_resumed = Arc::clone(&resumed_snapshots);
        let registration_reloaded = store_reloaded
            .register_query_listener(
                target_id,
                query.clone(),
                Arc::new(move |snapshot| {
                    callback_resumed.lock().unwrap().push(snapshot);
                }),
            )
            .await
            .expect("re-register listener");

        {
            let guard = resumed_snapshots.lock().unwrap();
            assert_eq!(guard.len(), 1);
            let snapshot = guard[0].clone();
            assert_eq!(snapshot.doc_changes().len(), 0);
            assert_eq!(snapshot.documents().len(), 1);
            assert_eq!(
                snapshot.resume_token().map(|token| token.to_vec()),
                Some(vec![1, 2, 3])
            );
            assert_eq!(
                snapshot.snapshot_version().map(|ts| (ts.seconds, ts.nanos)),
                Some((42, 0))
            );
            let doc = &snapshot.documents()[0];
            let data = doc.data().expect("document data");
            assert_eq!(
                data.get("name").cloned(),
                Some(FirestoreValue::from_string("San Francisco"))
            );
        }

        drop(registration);
        drop(registration_reloaded);
    }
}
