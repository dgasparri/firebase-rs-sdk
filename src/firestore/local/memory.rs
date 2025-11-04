use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Mutex as StdMutex;

use async_lock::Mutex;
use async_trait::async_trait;

use crate::firestore::error::{invalid_argument, FirestoreError, FirestoreResult};
use crate::firestore::model::{DocumentKey, Timestamp};
use crate::firestore::remote::datastore::WriteOperation;
use crate::firestore::remote::mutation::{MutationBatch, MutationBatchResult};
use crate::firestore::remote::remote_event::RemoteEvent;
use crate::firestore::remote::streams::WriteResult;
use crate::firestore::remote::syncer_bridge::{RemoteSyncerBridge, RemoteSyncerDelegate};
use crate::firestore::remote::watch_change::WatchDocument;

/// Minimal in-memory local store that cooperates with [`RemoteStore`] through
/// [`RemoteSyncerBridge`].
///
/// This structure mirrors the responsibilities of the Firestore JS `LocalStore`
/// required by the remote sync pipeline: it tracks remote document state,
/// remembers pending/acknowledged mutation batches, and surfaces stream tokens
/// and write results so higher layers can coordinate latency compensation.
#[derive(Debug)]
pub struct MemoryLocalStore {
    documents: Mutex<BTreeMap<DocumentKey, Option<WatchDocument>>>,
    remote_events: Mutex<Vec<RemoteEvent>>,
    rejected_targets: Mutex<Vec<(i32, FirestoreError)>>,
    successful_writes: Mutex<Vec<MutationBatchResult>>,
    failed_writes: Mutex<Vec<(i32, FirestoreError)>>,
    outstanding_batches: Mutex<Vec<i32>>,
    last_stream_token: StdMutex<Option<Vec<u8>>>,
    write_results: StdMutex<Vec<(i32, Vec<WriteResult>)>>,
    next_batch_id: AtomicI32,
}

impl MemoryLocalStore {
    /// Builds an empty memory-backed local store.
    pub fn new() -> Self {
        Self {
            documents: Mutex::new(BTreeMap::new()),
            remote_events: Mutex::new(Vec::new()),
            rejected_targets: Mutex::new(Vec::new()),
            successful_writes: Mutex::new(Vec::new()),
            failed_writes: Mutex::new(Vec::new()),
            outstanding_batches: Mutex::new(Vec::new()),
            last_stream_token: StdMutex::new(None),
            write_results: StdMutex::new(Vec::new()),
            next_batch_id: AtomicI32::new(1),
        }
    }

    /// Queues a mutation batch for remote delivery via the supplied bridge.
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
        bridge.enqueue_batch(batch).await?;
        self.outstanding_batches.lock().await.push(batch_id);
        Ok(batch_id)
    }

    /// Seeds the bridge with known remote keys for the provided target.
    pub fn replace_remote_keys(
        &self,
        bridge: &RemoteSyncerBridge<Self>,
        target_id: i32,
        keys: BTreeSet<DocumentKey>,
    ) {
        bridge.replace_remote_keys(target_id, keys);
    }

    /// Returns the most recent remote event applied to the store.
    pub async fn last_remote_event(&self) -> Option<RemoteEvent> {
        self.remote_events.lock().await.last().cloned()
    }

    /// Retrieves the tracked document for `key`, if any.
    pub async fn document(&self, key: &DocumentKey) -> Option<Option<WatchDocument>> {
        self.documents.lock().await.get(key).cloned()
    }

    /// Lists mutation batch ids acknowledged by the backend.
    pub async fn successful_batch_ids(&self) -> Vec<i32> {
        self.successful_writes
            .lock()
            .await
            .iter()
            .map(|result| result.batch_id())
            .collect()
    }

    /// Lists ids of batches rejected by the backend.
    pub async fn failed_batch_ids(&self) -> Vec<i32> {
        self.failed_writes
            .lock()
            .await
            .iter()
            .map(|(batch_id, _)| *batch_id)
            .collect()
    }

    /// Returns ids for batches that are still pending delivery.
    pub async fn outstanding_batch_ids(&self) -> Vec<i32> {
        self.outstanding_batches.lock().await.clone()
    }

    /// Returns a clone of the latest stream token delivered by the backend.
    pub fn last_stream_token(&self) -> Option<Vec<u8>> {
        self.last_stream_token.lock().unwrap().clone()
    }

    /// Returns the write results recorded so far.
    pub fn recorded_write_results(&self) -> Vec<(i32, Vec<WriteResult>)> {
        self.write_results.lock().unwrap().clone()
    }

    async fn clear_all(&self) {
        self.documents.lock().await.clear();
        self.remote_events.lock().await.clear();
        self.rejected_targets.lock().await.clear();
        self.successful_writes.lock().await.clear();
        self.failed_writes.lock().await.clear();
        self.outstanding_batches.lock().await.clear();
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

        self.remote_events.lock().await.push(event);
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
        self.successful_writes.lock().await.push(result);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
    use base64::Engine;
    use serde_json::json;
    use std::sync::Arc;

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

        // Consume addTarget handshake
        let add_target = listen_stream
            .next()
            .await
            .expect("addTarget frame")
            .expect("payload");
        let add_json: serde_json::Value = serde_json::from_slice(&add_target).unwrap();
        assert!(add_json.get("addTarget").is_some());

        // Deliver a document change
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

        // Queue a delete write
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
    }
}
