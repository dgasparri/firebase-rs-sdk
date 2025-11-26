use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex as StdMutex};

use async_lock::Mutex;
use async_trait::async_trait;

use crate::firestore::error::{internal_error, FirestoreError, FirestoreResult};
use crate::firestore::model::{DocumentKey, Timestamp};
use crate::firestore::remote::mutation::{MutationBatch, MutationBatchResult};
use crate::firestore::remote::remote_event::RemoteEvent;
use crate::firestore::remote::remote_syncer::{box_remote_store_future, RemoteStoreFuture, RemoteSyncer};
use crate::firestore::remote::streams::write::WriteResult;

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait RemoteSyncerDelegate: Send + Sync + 'static {
    async fn handle_remote_event(&self, event: RemoteEvent) -> FirestoreResult<()>;
    async fn handle_rejected_listen(&self, target_id: i32, error: FirestoreError) -> FirestoreResult<()>;
    async fn handle_successful_write(&self, result: MutationBatchResult) -> FirestoreResult<()>;
    async fn handle_failed_write(&self, batch_id: i32, error: FirestoreError) -> FirestoreResult<()>;

    async fn handle_credential_change(&self) -> FirestoreResult<()> {
        Ok(())
    }

    fn notify_stream_token_change(&self, _token: Option<Vec<u8>>) {}

    fn record_write_results(&self, _batch_id: i32, _results: &[WriteResult]) {}

    fn update_target_metadata(&self, _target_id: i32, _update: TargetMetadataUpdate) {}

    fn reset_target_metadata(&self, _target_id: i32) {}

    fn record_resolved_limbo_documents(&self, _documents: &BTreeSet<DocumentKey>) {}
}

#[derive(Clone, Debug, Default)]
pub struct TargetMetadataUpdate {
    pub resume_token: Option<Vec<u8>>,
    pub snapshot_version: Option<Timestamp>,
    pub current: bool,
    pub added_documents: BTreeSet<DocumentKey>,
    pub modified_documents: BTreeSet<DocumentKey>,
    pub removed_documents: BTreeSet<DocumentKey>,
}

impl TargetMetadataUpdate {
    pub fn from_change(
        change: &crate::firestore::remote::remote_event::TargetChange,
        snapshot: Option<Timestamp>,
    ) -> Self {
        Self {
            resume_token: change.resume_token.clone(),
            snapshot_version: snapshot,
            current: change.current,
            added_documents: change.added_documents.clone(),
            modified_documents: change.modified_documents.clone(),
            removed_documents: change.removed_documents.clone(),
        }
    }
}

/// Concrete `RemoteSyncer` implementation that bridges the remote store façade
/// with higher layers (local store, query views, etc.).
///
/// The bridge keeps track of remote document keys per target so watch change
/// aggregation mirrors the JS SDK, manages the pending mutation queue that feeds
/// the write stream, and delegates actual data processing to the provided
/// [`RemoteSyncerDelegate`].
pub struct RemoteSyncerBridge<D>
where
    D: RemoteSyncerDelegate,
{
    delegate: Arc<D>,
    remote_keys: StdMutex<BTreeMap<i32, BTreeSet<DocumentKey>>>,
    mutation_queue: Mutex<MutationQueue>,
}

impl<D> RemoteSyncerBridge<D>
where
    D: RemoteSyncerDelegate,
{
    pub fn new(delegate: Arc<D>) -> Self {
        Self {
            delegate,
            remote_keys: StdMutex::new(BTreeMap::new()),
            mutation_queue: Mutex::new(MutationQueue::default()),
        }
    }

    pub fn delegate(&self) -> Arc<D> {
        Arc::clone(&self.delegate)
    }

    pub fn seed_remote_keys(&self, target_id: i32, keys: BTreeSet<DocumentKey>) {
        let mut guard = self.remote_keys.lock().unwrap();
        guard.insert(target_id, keys);
    }

    pub fn replace_remote_keys(&self, target_id: i32, keys: BTreeSet<DocumentKey>) {
        {
            let mut guard = self.remote_keys.lock().unwrap();
            guard.insert(target_id, keys.clone());
        }

        self.delegate.reset_target_metadata(target_id);
        if !keys.is_empty() {
            let mut update = TargetMetadataUpdate::default();
            update.added_documents = keys;
            self.delegate.update_target_metadata(target_id, update);
        }
    }

    pub fn clear_remote_keys(&self, target_id: i32) {
        let mut guard = self.remote_keys.lock().unwrap();
        guard.remove(&target_id);
    }

    pub async fn enqueue_batch(&self, batch: MutationBatch) -> FirestoreResult<()> {
        let mut guard = self.mutation_queue.lock().await;
        guard.enqueue(batch)
    }

    pub async fn pending_batch_ids(&self) -> Vec<i32> {
        let guard = self.mutation_queue.lock().await;
        guard.batches.iter().map(|batch| batch.batch_id).collect()
    }

    pub fn remote_keys_for_target(&self, target_id: i32) -> BTreeSet<DocumentKey> {
        self.remote_keys
            .lock()
            .unwrap()
            .get(&target_id)
            .cloned()
            .unwrap_or_default()
    }

    fn update_remote_keys_from_event(&self, event: &RemoteEvent) {
        let mut pending_updates: Vec<(i32, TargetMetadataUpdate)> = Vec::new();
        {
            let mut guard = self.remote_keys.lock().unwrap();
            for (target_id, change) in &event.target_changes {
                let keys = guard.entry(*target_id).or_default();
                for key in &change.removed_documents {
                    keys.remove(key);
                }
                for key in change.added_documents.iter().chain(change.modified_documents.iter()) {
                    keys.insert(key.clone());
                }
                pending_updates.push((*target_id, TargetMetadataUpdate::from_change(change, event.snapshot_version)));
            }

            for target_id in &event.target_resets {
                guard.remove(target_id);
            }
        }

        for (target_id, update) in pending_updates {
            self.delegate.update_target_metadata(target_id, update);
        }

        for target_id in &event.target_resets {
            self.delegate.reset_target_metadata(*target_id);
        }

        if !event.resolved_limbo_documents.is_empty() {
            self.delegate
                .record_resolved_limbo_documents(&event.resolved_limbo_documents);
        }
    }
}

impl<D> RemoteSyncer for RemoteSyncerBridge<D>
where
    D: RemoteSyncerDelegate,
{
    fn apply_remote_event(&self, event: RemoteEvent) -> RemoteStoreFuture<'_, FirestoreResult<()>> {
        self.update_remote_keys_from_event(&event);
        let delegate = Arc::clone(&self.delegate);
        box_remote_store_future(async move { delegate.handle_remote_event(event).await })
    }

    fn reject_listen(&self, target_id: i32, error: FirestoreError) -> RemoteStoreFuture<'_, FirestoreResult<()>> {
        self.clear_remote_keys(target_id);
        let delegate = Arc::clone(&self.delegate);
        box_remote_store_future(async move { delegate.handle_rejected_listen(target_id, error).await })
    }

    fn apply_successful_write(&self, result: MutationBatchResult) -> RemoteStoreFuture<'_, FirestoreResult<()>> {
        let queue = &self.mutation_queue;
        let batch_id = result.batch_id();
        let delegate = Arc::clone(&self.delegate);
        box_remote_store_future(async move {
            {
                let mut guard = queue.lock().await;
                guard.remove(batch_id)?;
            }
            delegate.handle_successful_write(result).await
        })
    }

    fn reject_failed_write(&self, batch_id: i32, error: FirestoreError) -> RemoteStoreFuture<'_, FirestoreResult<()>> {
        let queue = &self.mutation_queue;
        let delegate = Arc::clone(&self.delegate);
        box_remote_store_future(async move {
            {
                let mut guard = queue.lock().await;
                guard.remove(batch_id)?;
            }
            delegate.handle_failed_write(batch_id, error).await
        })
    }

    fn get_remote_keys_for_target(&self, target_id: i32) -> BTreeSet<DocumentKey> {
        self.remote_keys
            .lock()
            .unwrap()
            .get(&target_id)
            .cloned()
            .unwrap_or_default()
    }

    fn next_mutation_batch(
        &self,
        after_batch_id: Option<i32>,
    ) -> RemoteStoreFuture<'_, FirestoreResult<Option<MutationBatch>>> {
        let queue = &self.mutation_queue;
        box_remote_store_future(async move {
            let guard = queue.lock().await;
            Ok(guard.next_after(after_batch_id))
        })
    }

    fn handle_credential_change(&self) -> RemoteStoreFuture<'_, FirestoreResult<()>> {
        let delegate = Arc::clone(&self.delegate);
        box_remote_store_future(async move { delegate.handle_credential_change().await })
    }

    fn notify_stream_token_change(&self, token: Option<Vec<u8>>) {
        self.delegate.notify_stream_token_change(token);
    }

    fn record_write_results(&self, batch_id: i32, results: &[WriteResult]) {
        self.delegate.record_write_results(batch_id, results);
    }
}

#[derive(Default)]
struct MutationQueue {
    batches: Vec<MutationBatch>,
}

impl MutationQueue {
    fn enqueue(&mut self, batch: MutationBatch) -> FirestoreResult<()> {
        match self
            .batches
            .binary_search_by_key(&batch.batch_id, |existing| existing.batch_id)
        {
            Ok(_) => Err(internal_error(format!("Duplicate mutation batch id {} queued", batch.batch_id))),
            Err(pos) => {
                self.batches.insert(pos, batch);
                Ok(())
            }
        }
    }

    fn remove(&mut self, batch_id: i32) -> FirestoreResult<MutationBatch> {
        match self
            .batches
            .binary_search_by_key(&batch_id, |existing| existing.batch_id)
        {
            Ok(pos) => Ok(self.batches.remove(pos)),
            Err(_) => Err(internal_error(format!("Mutation batch {batch_id} not found in queue"))),
        }
    }

    fn next_after(&self, after: Option<i32>) -> Option<MutationBatch> {
        let threshold = after.unwrap_or(-1);
        self.batches.iter().find(|batch| batch.batch_id > threshold).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firestore::model::{DocumentKey, Timestamp};
    use crate::firestore::remote::datastore::WriteOperation;
    use crate::firestore::value::MapValue;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct RecordingDelegate {
        events: StdMutex<Vec<RemoteEvent>>,
        rejected: StdMutex<Vec<i32>>,
        successes: StdMutex<Vec<i32>>,
        failures: StdMutex<Vec<i32>>,
        target_updates: StdMutex<Vec<(i32, TargetMetadataUpdate)>>,
        target_resets: StdMutex<Vec<i32>>,
        limbo_resolutions: StdMutex<Vec<BTreeSet<DocumentKey>>>,
    }

    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl RemoteSyncerDelegate for RecordingDelegate {
        async fn handle_remote_event(&self, event: RemoteEvent) -> FirestoreResult<()> {
            self.events.lock().unwrap().push(event);
            Ok(())
        }

        async fn handle_rejected_listen(&self, target_id: i32, _error: FirestoreError) -> FirestoreResult<()> {
            self.rejected.lock().unwrap().push(target_id);
            Ok(())
        }

        async fn handle_successful_write(&self, result: MutationBatchResult) -> FirestoreResult<()> {
            self.successes.lock().unwrap().push(result.batch_id());
            Ok(())
        }

        async fn handle_failed_write(&self, batch_id: i32, _error: FirestoreError) -> FirestoreResult<()> {
            self.failures.lock().unwrap().push(batch_id);
            Ok(())
        }

        fn update_target_metadata(&self, target_id: i32, update: TargetMetadataUpdate) {
            self.target_updates.lock().unwrap().push((target_id, update));
        }

        fn reset_target_metadata(&self, target_id: i32) {
            self.target_resets.lock().unwrap().push(target_id);
        }

        fn record_resolved_limbo_documents(&self, documents: &BTreeSet<DocumentKey>) {
            self.limbo_resolutions.lock().unwrap().push(documents.clone());
        }
    }

    fn sample_write(id: &str) -> WriteOperation {
        let key = DocumentKey::from_string(&format!("cities/{id}")).unwrap();
        WriteOperation::Set {
            key,
            data: MapValue::new(BTreeMap::new()),
            mask: None,
            transforms: Vec::new(),
        }
    }

    fn sample_batch(batch_id: i32, doc_id: &str) -> MutationBatch {
        MutationBatch::from_writes(batch_id, Timestamp::now(), vec![sample_write(doc_id)])
    }

    #[tokio::test]
    async fn queue_returns_batches_in_order() {
        let delegate = Arc::new(RecordingDelegate::default());
        let bridge = RemoteSyncerBridge::new(delegate);
        bridge.enqueue_batch(sample_batch(10, "a")).await.unwrap();
        bridge.enqueue_batch(sample_batch(5, "b")).await.unwrap();

        let first = bridge.next_mutation_batch(None).await.unwrap().expect("first batch");
        assert_eq!(first.batch_id, 5);
        let second = bridge
            .next_mutation_batch(Some(5))
            .await
            .unwrap()
            .expect("second batch");
        assert_eq!(second.batch_id, 10);
    }

    #[tokio::test]
    async fn acknowledges_batches_remove_from_queue() {
        let delegate = Arc::new(RecordingDelegate::default());
        let bridge = RemoteSyncerBridge::new(delegate.clone());
        bridge.enqueue_batch(sample_batch(1, "a")).await.unwrap();

        let batch = bridge.next_mutation_batch(None).await.unwrap().unwrap();
        let result = MutationBatchResult::from(
            batch.clone(),
            None,
            vec![WriteResult {
                update_time: None,
                transform_results: Vec::new(),
            }],
        )
        .unwrap();
        bridge.apply_successful_write(result).await.unwrap();

        assert!(bridge.next_mutation_batch(None).await.unwrap().is_none());
        assert_eq!(delegate.successes.lock().unwrap().as_slice(), &[1]);
    }

    #[tokio::test]
    async fn apply_remote_event_updates_keys() {
        let delegate = Arc::new(RecordingDelegate::default());
        let bridge = RemoteSyncerBridge::new(delegate);
        let key = DocumentKey::from_string("cities/sf").unwrap();
        let mut change = crate::firestore::remote::remote_event::TargetChange::default();
        change.added_documents.insert(key.clone());

        let mut event = RemoteEvent::default();
        event.target_changes.insert(2, change);
        bridge.apply_remote_event(event).await.unwrap();

        let keys = bridge.get_remote_keys_for_target(2);
        assert!(keys.contains(&key));
    }

    #[tokio::test]
    async fn rejected_listen_clears_keys() {
        let delegate = Arc::new(RecordingDelegate::default());
        let bridge = RemoteSyncerBridge::new(delegate.clone());
        bridge.replace_remote_keys(4, BTreeSet::from([DocumentKey::from_string("cities/sf").unwrap()]));

        bridge.reject_listen(4, internal_error("boom")).await.unwrap();
        assert!(bridge.get_remote_keys_for_target(4).is_empty());
        assert_eq!(delegate.rejected.lock().unwrap().as_slice(), &[4]);
    }

    #[tokio::test]
    async fn failed_write_is_reported() {
        let delegate = Arc::new(RecordingDelegate::default());
        let bridge = RemoteSyncerBridge::new(delegate.clone());
        bridge.enqueue_batch(sample_batch(3, "x")).await.unwrap();

        bridge.reject_failed_write(3, internal_error("fail")).await.unwrap();
        assert!(bridge.next_mutation_batch(None).await.unwrap().is_none());
        assert_eq!(delegate.failures.lock().unwrap().as_slice(), &[3]);
    }
}
