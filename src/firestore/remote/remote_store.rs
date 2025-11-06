use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Weak};

use async_lock::Mutex;
use async_trait::async_trait;

use crate::firestore::error::{internal_error, FirestoreError, FirestoreResult};
use crate::firestore::model::Timestamp;
use crate::firestore::remote::mutation::{MutationBatch, MutationBatchResult};
use crate::firestore::remote::remote_syncer::RemoteSyncer;
use crate::firestore::remote::streams::{
    ListenStream, ListenStreamDelegate, ListenTarget, WriteResponse, WriteStream,
    WriteStreamDelegate,
};
use crate::firestore::remote::watch_change::{
    DocumentDelete, DocumentRemove, WatchChange, WatchTargetChange,
};
use crate::firestore::remote::{
    JsonProtoSerializer, NetworkLayer, TargetMetadataProvider, WatchChangeAggregator,
};

#[cfg(test)]
use crate::firestore::remote::remote_event::RemoteEvent;

const MAX_PENDING_WRITES: usize = 10;

/// Enumerates reasons why the remote store temporarily disables network usage.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum OfflineCause {
    UserDisabled,
    CredentialChange,
    ConnectivityChange,
    Shutdown,
}

struct SyncerMetadataProvider {
    syncer: Arc<dyn RemoteSyncer>,
}

impl SyncerMetadataProvider {
    fn new(syncer: Arc<dyn RemoteSyncer>) -> Self {
        Self { syncer }
    }
}

impl TargetMetadataProvider for SyncerMetadataProvider {
    fn get_remote_keys(&self, target_id: i32) -> BTreeSet<crate::firestore::model::DocumentKey> {
        self.syncer.get_remote_keys_for_target(target_id)
    }
}

struct RemoteStoreState {
    listen_targets: BTreeMap<i32, ListenTarget>,
    watch_stream: Option<Arc<ListenStream<RemoteListenDelegate>>>,
    write_stream: Option<Arc<WriteStream<RemoteWriteDelegate>>>,
    watch_aggregator: Option<WatchChangeAggregator<SyncerMetadataProvider>>,
    write_handshake_complete: bool,
    write_pipeline: VecDeque<MutationBatch>,
    last_batch_id: Option<i32>,
    offline_causes: BTreeSet<OfflineCause>,
}

impl Default for RemoteStoreState {
    fn default() -> Self {
        Self {
            listen_targets: BTreeMap::new(),
            watch_stream: None,
            write_stream: None,
            watch_aggregator: None,
            write_handshake_complete: false,
            write_pipeline: VecDeque::new(),
            last_batch_id: None,
            offline_causes: BTreeSet::new(),
        }
    }
}

/// Coordinates watch and write streams just like the JS RemoteStore facade.
///
/// The implementation mirrors the TypeScript logic in
/// `packages/firestore/src/remote/remote_store.ts`, but adapts the control flow
/// to Rust's async/await style and the streaming primitives already ported to
/// this repository.
#[derive(Clone)]
pub struct RemoteStore {
    inner: Arc<RemoteStoreInner>,
}

impl RemoteStore {
    /// Builds a new remote store using the provided network layer and
    /// serializer.
    pub fn new(
        network_layer: NetworkLayer,
        serializer: JsonProtoSerializer,
        remote_syncer: Arc<dyn RemoteSyncer>,
    ) -> Self {
        let inner = Arc::new(RemoteStoreInner::new(
            network_layer,
            serializer,
            remote_syncer,
        ));
        Self { inner }
    }

    /// Enables network usage and restarts the streams if necessary.
    pub async fn enable_network(&self) -> FirestoreResult<()> {
        self.inner.enable_network().await
    }

    /// Temporarily disables all remote streams.
    pub async fn disable_network(&self) -> FirestoreResult<()> {
        self.inner.disable_network(OfflineCause::UserDisabled).await
    }

    /// Shuts the remote store down permanently.
    pub async fn shutdown(&self) -> FirestoreResult<()> {
        self.inner.shutdown().await
    }

    /// Registers a new listen target. If the listen stream is active, the
    /// target is sent immediately; otherwise it will be sent after the next
    /// reconnect.
    pub async fn listen(&self, target: ListenTarget) -> FirestoreResult<()> {
        self.inner.listen(target).await
    }

    /// Removes an existing listen target from the active set.
    pub async fn unlisten(&self, target_id: i32) -> FirestoreResult<()> {
        self.inner.unlisten(target_id).await
    }

    /// Asks the remote store to poll the mutation queue and push pending
    /// batches to the write stream.
    pub async fn pump_writes(&self) -> FirestoreResult<()> {
        self.inner.fill_write_pipeline().await
    }

    /// Notifies the remote store that authentication credentials changed,
    /// forcing a stream restart.
    pub async fn handle_credential_change(&self) -> FirestoreResult<()> {
        self.inner.handle_credential_change().await
    }
}

struct RemoteStoreInner {
    state: Mutex<RemoteStoreState>,
    network_layer: NetworkLayer,
    serializer: JsonProtoSerializer,
    remote_syncer: Arc<dyn RemoteSyncer>,
}

impl RemoteStoreInner {
    fn new(
        network_layer: NetworkLayer,
        serializer: JsonProtoSerializer,
        remote_syncer: Arc<dyn RemoteSyncer>,
    ) -> Self {
        Self {
            state: Mutex::new(RemoteStoreState::default()),
            network_layer,
            serializer,
            remote_syncer,
        }
    }

    async fn enable_network(self: &Arc<Self>) -> FirestoreResult<()> {
        {
            let mut state = self.state.lock().await;
            state.offline_causes.remove(&OfflineCause::UserDisabled);
        }
        self.ensure_streams().await
    }

    async fn disable_network(self: &Arc<Self>, cause: OfflineCause) -> FirestoreResult<()> {
        let streams = {
            let mut state = self.state.lock().await;
            state.offline_causes.insert(cause);
            Self::take_streams_locked(&mut state)
        };
        self.stop_streams(streams);
        Ok(())
    }

    async fn shutdown(self: &Arc<Self>) -> FirestoreResult<()> {
        let streams = {
            let mut state = self.state.lock().await;
            state.offline_causes.insert(OfflineCause::Shutdown);
            Self::take_streams_locked(&mut state)
        };
        self.stop_streams(streams);
        Ok(())
    }

    async fn listen(self: &Arc<Self>, target: ListenTarget) -> FirestoreResult<()> {
        let target_id = target.target_id();
        let (stream, should_start) = {
            let mut state = self.state.lock().await;
            if state.listen_targets.contains_key(&target_id) {
                return Ok(());
            }
            state.listen_targets.insert(target_id, target.clone());
            let stream = state.watch_stream.clone();
            let should_start = stream.is_none() && Self::can_use_network_locked(&state);
            (stream, should_start)
        };

        if let Some(stream) = stream {
            stream.watch(target).await
        } else if should_start {
            self.start_watch_stream().await
        } else {
            Ok(())
        }
    }

    async fn unlisten(self: &Arc<Self>, target_id: i32) -> FirestoreResult<()> {
        let stream = {
            let mut state = self.state.lock().await;
            state.listen_targets.remove(&target_id);
            state.watch_stream.clone()
        };
        if let Some(stream) = stream {
            stream.unwatch(target_id).await?
        }
        Ok(())
    }

    async fn handle_credential_change(self: &Arc<Self>) -> FirestoreResult<()> {
        self.remote_syncer.handle_credential_change().await?;
        {
            let mut state = self.state.lock().await;
            state.offline_causes.insert(OfflineCause::CredentialChange);
            let streams = Self::take_streams_locked(&mut state);
            drop(streams);
        }
        self.enable_network().await
    }

    async fn ensure_streams(self: &Arc<Self>) -> FirestoreResult<()> {
        self.start_watch_stream().await?;
        self.start_write_stream().await?;
        self.fill_write_pipeline().await
    }

    async fn start_watch_stream(self: &Arc<Self>) -> FirestoreResult<()> {
        let (stream, targets) = {
            let mut state = self.state.lock().await;
            if !Self::can_use_network_locked(&state) {
                return Ok(());
            }
            if state.watch_stream.is_some() || state.listen_targets.is_empty() {
                return Ok(());
            }
            let provider = Arc::new(SyncerMetadataProvider::new(Arc::clone(&self.remote_syncer)));
            state.watch_aggregator = Some(WatchChangeAggregator::new(provider));
            let delegate = Arc::new(RemoteListenDelegate::new(Arc::downgrade(self)));
            let stream = Arc::new(ListenStream::new(
                self.network_layer.clone(),
                self.serializer.clone(),
                delegate,
            ));
            let targets = state.listen_targets.values().cloned().collect::<Vec<_>>();
            state.watch_stream = Some(stream.clone());
            (stream, targets)
        };

        for target in targets {
            stream.watch(target).await?;
        }
        Ok(())
    }

    async fn start_write_stream(self: &Arc<Self>) -> FirestoreResult<()> {
        {
            let mut state = self.state.lock().await;
            if !Self::can_use_network_locked(&state)
                || state.write_stream.is_some()
                || state.write_pipeline.is_empty()
            {
                return Ok(());
            }
            state.write_handshake_complete = false;
            let delegate = Arc::new(RemoteWriteDelegate::new(Arc::downgrade(self)));
            let stream = Arc::new(WriteStream::new(
                self.network_layer.clone(),
                self.serializer.clone(),
                delegate,
            ));
            state.write_stream = Some(stream);
        }
        Ok(())
    }

    async fn fill_write_pipeline(self: &Arc<Self>) -> FirestoreResult<()> {
        loop {
            let (should_fetch, last_batch_id) = {
                let state = self.state.lock().await;
                (
                    Self::can_use_network_locked(&state)
                        && state.write_pipeline.len() < MAX_PENDING_WRITES,
                    state.last_batch_id,
                )
            };

            if !should_fetch {
                break;
            }

            let maybe_batch = self
                .remote_syncer
                .next_mutation_batch(last_batch_id)
                .await?;
            let batch = match maybe_batch {
                Some(batch) if !batch.is_empty() => batch,
                _ => break,
            };

            let writes = batch.writes.clone();
            let (stream, handshake_complete) = {
                let mut state = self.state.lock().await;
                state.last_batch_id = Some(batch.batch_id);
                state.write_pipeline.push_back(batch);
                (state.write_stream.clone(), state.write_handshake_complete)
            };

            if let Some(stream) = stream {
                if handshake_complete {
                    stream.write(writes).await?;
                }
            } else {
                self.start_write_stream().await?;
            }
        }
        Ok(())
    }

    async fn on_watch_change(self: &Arc<Self>, change: WatchChange) -> FirestoreResult<()> {
        if let WatchChange::TargetChange(target_change) = &change {
            if let Some(error) = target_change.cause.clone() {
                return self.handle_target_error(target_change.clone(), error).await;
            }
        }

        let event = {
            let mut state = self.state.lock().await;
            let aggregator = match state.watch_aggregator.as_mut() {
                Some(aggregator) => aggregator,
                None => return Ok(()),
            };

            if let Some(version) = snapshot_version_for_change(&change) {
                aggregator.set_snapshot_version(Some(version));
            }

            aggregator.handle_watch_change(change)?;
            aggregator.drain()
        };

        if !event.is_empty() {
            let target_resets: Vec<i32> = event.target_resets.iter().copied().collect();
            if !target_resets.is_empty() {
                self.handle_target_resets(&target_resets).await?;
            }
            self.remote_syncer.apply_remote_event(event).await?;
        }
        Ok(())
    }

    async fn on_watch_error(self: &Arc<Self>, error: FirestoreError) {
        {
            let mut state = self.state.lock().await;
            let streams = Self::take_watch_stream_locked(&mut state);
            drop(streams);
        }
        if let Err(err) = self.start_watch_stream().await {
            log::warn!("failed to restart watch stream after error: {err}");
        }
        log::warn!("watch stream error: {error}");
    }

    async fn on_write_handshake_complete(self: &Arc<Self>) -> FirestoreResult<()> {
        let batches = {
            let mut state = self.state.lock().await;
            state.write_handshake_complete = true;
            state
                .write_pipeline
                .iter()
                .map(|b| b.writes.clone())
                .collect::<Vec<_>>()
        };

        let stream = {
            let state = self.state.lock().await;
            state.write_stream.clone()
        };

        if let Some(stream) = stream {
            for writes in batches {
                stream.write(writes).await?;
            }
        }
        Ok(())
    }

    async fn on_write_response(self: &Arc<Self>, response: WriteResponse) -> FirestoreResult<()> {
        let batch = {
            let mut state = self.state.lock().await;
            state
                .write_pipeline
                .pop_front()
                .ok_or_else(|| internal_error("write pipeline empty"))?
        };

        self.remote_syncer
            .record_write_results(batch.batch_id, &response.write_results);
        self.remote_syncer
            .notify_stream_token_change(Some(response.stream_token.clone()));

        let result =
            MutationBatchResult::from(batch, response.commit_time, response.write_results)?;
        self.remote_syncer.apply_successful_write(result).await?;
        self.fill_write_pipeline().await
    }

    async fn on_write_error(self: &Arc<Self>, error: FirestoreError) {
        {
            let mut state = self.state.lock().await;
            if let Some(stream) = state.write_stream.take() {
                stream.stop();
            }
            state.write_handshake_complete = false;
        }
        if let Err(err) = self.start_write_stream().await {
            log::warn!("failed to restart write stream after error: {err}");
        }
        log::warn!("write stream error: {error}");
    }

    async fn handle_target_error(
        self: &Arc<Self>,
        change: WatchTargetChange,
        error: FirestoreError,
    ) -> FirestoreResult<()> {
        for target_id in change.target_ids {
            self.remote_syncer
                .reject_listen(target_id, error.clone())
                .await?;
        }
        {
            let mut state = self.state.lock().await;
            state.watch_aggregator = None;
        }
        self.start_watch_stream().await
    }

    fn take_streams_locked(
        state: &mut RemoteStoreState,
    ) -> (
        Option<Arc<ListenStream<RemoteListenDelegate>>>,
        Option<Arc<WriteStream<RemoteWriteDelegate>>>,
    ) {
        let watch = Self::take_watch_stream_locked(state);
        let write = state.write_stream.take();
        state.write_handshake_complete = false;
        (watch, write)
    }

    fn take_watch_stream_locked(
        state: &mut RemoteStoreState,
    ) -> Option<Arc<ListenStream<RemoteListenDelegate>>> {
        state.watch_aggregator = None;
        state.watch_stream.take()
    }

    fn stop_streams(
        &self,
        streams: (
            Option<Arc<ListenStream<RemoteListenDelegate>>>,
            Option<Arc<WriteStream<RemoteWriteDelegate>>>,
        ),
    ) {
        if let Some(stream) = streams.0 {
            stream.stop();
        }
        if let Some(stream) = streams.1 {
            stream.stop();
        }
    }

    fn can_use_network_locked(state: &RemoteStoreState) -> bool {
        state.offline_causes.is_empty()
    }

    async fn handle_target_resets(self: &Arc<Self>, target_ids: &[i32]) -> FirestoreResult<()> {
        let (targets, stream_available) = {
            let state = self.state.lock().await;
            let stream = state.watch_stream.clone();
            let targets: Vec<(i32, ListenTarget)> = target_ids
                .iter()
                .filter_map(|id| {
                    state
                        .listen_targets
                        .get(id)
                        .cloned()
                        .map(|target| (*id, target))
                })
                .collect();
            (targets, stream)
        };

        if targets.is_empty() {
            return Ok(());
        }

        if let Some(stream) = stream_available {
            for (target_id, target) in targets {
                stream.unwatch(target_id).await?;
                stream.watch(target).await?;
            }
            Ok(())
        } else {
            self.start_watch_stream().await
        }
    }
}

struct RemoteListenDelegate {
    inner: Weak<RemoteStoreInner>,
}

impl RemoteListenDelegate {
    fn new(inner: Weak<RemoteStoreInner>) -> Self {
        Self { inner }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl ListenStreamDelegate for RemoteListenDelegate {
    async fn on_watch_change(&self, change: WatchChange) -> FirestoreResult<()> {
        if let Some(inner) = self.inner.upgrade() {
            inner.on_watch_change(change).await
        } else {
            Ok(())
        }
    }

    async fn on_stream_error(&self, error: FirestoreError) {
        if let Some(inner) = self.inner.upgrade() {
            inner.on_watch_error(error).await;
        }
    }
}

struct RemoteWriteDelegate {
    inner: Weak<RemoteStoreInner>,
}

impl RemoteWriteDelegate {
    fn new(inner: Weak<RemoteStoreInner>) -> Self {
        Self { inner }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl WriteStreamDelegate for RemoteWriteDelegate {
    async fn on_handshake_complete(&self) -> FirestoreResult<()> {
        if let Some(inner) = self.inner.upgrade() {
            inner.on_write_handshake_complete().await
        } else {
            Ok(())
        }
    }

    async fn on_write_response(&self, response: WriteResponse) -> FirestoreResult<()> {
        if let Some(inner) = self.inner.upgrade() {
            inner.on_write_response(response).await
        } else {
            Ok(())
        }
    }

    async fn on_stream_error(&self, error: FirestoreError) {
        if let Some(inner) = self.inner.upgrade() {
            inner.on_write_error(error).await;
        }
    }
}

fn snapshot_version_for_change(change: &WatchChange) -> Option<Timestamp> {
    match change {
        WatchChange::TargetChange(change) => change.read_time.clone(),
        WatchChange::DocumentDelete(DocumentDelete { read_time, .. }) => read_time.clone(),
        WatchChange::DocumentRemove(DocumentRemove { read_time, .. }) => read_time.clone(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firestore::{LimitType, QueryDefinition};
    use crate::firestore::model::{DatabaseId, ResourcePath};
    use crate::firestore::remote::datastore::streaming::StreamingDatastoreImpl;
    use crate::firestore::remote::datastore::{NoopTokenProvider, TokenProviderArc};
    use crate::firestore::remote::network::NetworkLayer;
    use crate::firestore::remote::remote_syncer::{box_remote_store_future, RemoteStoreFuture};
    use crate::firestore::remote::stream::{
        InMemoryTransport, MultiplexedConnection, MultiplexedStream,
    };
    use crate::firestore::remote::JsonProtoSerializer;
    use crate::platform::runtime;
    use async_trait::async_trait;
    use std::sync::Mutex as StdMutex;

    #[derive(Default)]
    struct TestRemoteSyncer {
        events: Mutex<Vec<RemoteEvent>>,
        rejected: Mutex<Vec<i32>>,
        writes: Mutex<Vec<MutationBatchResult>>,
        batches: Mutex<Vec<MutationBatch>>,
        remote_keys: StdMutex<BTreeMap<i32, BTreeSet<crate::firestore::model::DocumentKey>>>,
    }

    impl TestRemoteSyncer {
        fn new() -> Arc<Self> {
            Arc::new(Self::default())
        }
    }

    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl RemoteSyncer for TestRemoteSyncer {
        fn apply_remote_event(
            &self,
            event: RemoteEvent,
        ) -> RemoteStoreFuture<'_, FirestoreResult<()>> {
            box_remote_store_future(async move {
                self.events.lock().await.push(event);
                Ok(())
            })
        }

        fn reject_listen(
            &self,
            target_id: i32,
            _error: FirestoreError,
        ) -> RemoteStoreFuture<'_, FirestoreResult<()>> {
            box_remote_store_future(async move {
                self.rejected.lock().await.push(target_id);
                Ok(())
            })
        }

        fn apply_successful_write(
            &self,
            result: MutationBatchResult,
        ) -> RemoteStoreFuture<'_, FirestoreResult<()>> {
            box_remote_store_future(async move {
                self.writes.lock().await.push(result);
                Ok(())
            })
        }

        fn reject_failed_write(
            &self,
            batch_id: i32,
            _error: FirestoreError,
        ) -> RemoteStoreFuture<'_, FirestoreResult<()>> {
            box_remote_store_future(async move {
                self.rejected.lock().await.push(batch_id);
                Ok(())
            })
        }

        fn get_remote_keys_for_target(
            &self,
            target_id: i32,
        ) -> BTreeSet<crate::firestore::model::DocumentKey> {
            self.remote_keys
                .lock()
                .unwrap()
                .get(&target_id)
                .cloned()
                .unwrap_or_default()
        }

        fn next_mutation_batch(
            &self,
            _after_batch_id: Option<i32>,
        ) -> RemoteStoreFuture<'_, FirestoreResult<Option<MutationBatch>>> {
            box_remote_store_future(async move {
                let mut guard = self.batches.lock().await;
                Ok(guard.pop())
            })
        }
    }

    fn setup_remote_store(
        syncer: Arc<TestRemoteSyncer>,
    ) -> (RemoteStore, Arc<MultiplexedConnection>) {
        let (client_transport, server_transport) = InMemoryTransport::pair();
        let client_connection = Arc::new(MultiplexedConnection::new(client_transport));
        let server_connection = Arc::new(MultiplexedConnection::new(server_transport));

        let datastore = StreamingDatastoreImpl::new(Arc::clone(&client_connection));
        let network = NetworkLayer::builder(
            Arc::new(datastore) as Arc<_>,
            Arc::new(NoopTokenProvider::default()) as TokenProviderArc,
        )
        .build();
        let serializer = JsonProtoSerializer::new(DatabaseId::new("test", "(default)"));
        (
            RemoteStore::new(network, serializer, syncer),
            server_connection,
        )
    }

    fn sample_query_definition() -> QueryDefinition {
        QueryDefinition {
            collection_path: ResourcePath::from_string("cities").unwrap(),
            parent_path: ResourcePath::root(),
            collection_id: "cities".to_string(),
            collection_group: None,
            filters: Vec::new(),
            request_order_by: Vec::new(),
            result_order_by: Vec::new(),
            limit: None,
            limit_type: LimitType::First,
            request_start_at: None,
            request_end_at: None,
            result_start_at: None,
            result_end_at: None,
            projection: None,
        }
    }

    #[tokio::test]
    async fn listen_resends_targets_on_connect() {
        let syncer = TestRemoteSyncer::new();
        let (store, server_connection) = setup_remote_store(Arc::clone(&syncer));

        let serializer = JsonProtoSerializer::new(DatabaseId::new("test", "(default)"));
        let target = ListenTarget::for_query(&serializer, 1, &sample_query_definition())
            .expect("query target");

        store.enable_network().await.expect("enable network");
        store.listen(target.clone()).await.expect("listen target");

        let server_stream = server_connection
            .open_stream()
            .await
            .expect("server open listen");
        let frame = server_stream.next().await.expect("handshake");
        let payload = frame.expect("payload");
        let json: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert!(json.get("addTarget").is_some());

        let change = serde_json::json!({
            "documentChange": {
                "document": {
                    "name": "projects/test/databases/(default)/documents/cities/sf",
                    "fields": {}
                },
                "targetIds": [1],
                "removedTargetIds": []
            }
        });
        server_stream
            .send(serde_json::to_vec(&change).unwrap())
            .await
            .expect("send change");

        runtime::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(syncer.events.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn existence_filter_triggers_target_reset_and_relisten() {
        let syncer = TestRemoteSyncer::new();
        let (store, server_connection) = setup_remote_store(Arc::clone(&syncer));

        let serializer = JsonProtoSerializer::new(DatabaseId::new("test", "(default)"));
        let target = ListenTarget::for_query(&serializer, 1, &sample_query_definition())
            .expect("query target");

        store.enable_network().await.expect("enable network");
        store.listen(target.clone()).await.expect("listen target");

        let server_stream = server_connection
            .open_stream()
            .await
            .expect("server open listen");

        let first_frame = server_stream.next().await.expect("handshake");
        let payload = first_frame.expect("payload");
        let json: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert!(json.get("addTarget").is_some());

        let existence_filter = serde_json::json!({
            "filter": {
                "targetId": 1,
                "count": 0
            }
        });
        server_stream
            .send(serde_json::to_vec(&existence_filter).unwrap())
            .await
            .expect("send existence filter");

        runtime::sleep(std::time::Duration::from_millis(50)).await;

        let mut saw_remove = false;
        let mut saw_add = false;
        for _ in 0..4 {
            if let Some(frame) = server_stream.next().await {
                if let Ok(payload) = frame {
                    let json: serde_json::Value = serde_json::from_slice(&payload).unwrap();
                    if json.get("removeTarget").is_some() {
                        saw_remove = true;
                    }
                    if json.get("addTarget").is_some() {
                        saw_add = true;
                        break;
                    }
                }
            }
        }

        assert!(
            saw_add,
            "expected query to be re-listened after existence filter reset"
        );

        if !saw_remove {
            log::debug!("re-listen completed without explicit removeTarget frame");
        }

        let events = syncer.events.lock().await;
        assert!(
            events.iter().any(|event| event.target_resets.contains(&1)),
            "expected remote event with target reset"
        );
    }

    #[tokio::test]
    async fn existence_filter_resets_multiple_targets() {
        let syncer = TestRemoteSyncer::new();
        let (store, server_connection) = setup_remote_store(Arc::clone(&syncer));

        let serializer = JsonProtoSerializer::new(DatabaseId::new("test", "(default)"));
        let target1 = ListenTarget::for_query(&serializer, 1, &sample_query_definition())
            .expect("query target 1");
        let target2 = ListenTarget::for_query(&serializer, 2, &sample_query_definition())
            .expect("query target 2");

        store.enable_network().await.expect("enable network");
        store.listen(target1.clone()).await.expect("listen target1");
        store.listen(target2.clone()).await.expect("listen target2");

        let server_stream = server_connection
            .open_stream()
            .await
            .expect("server open listen");

        // Drain initial addTarget frames.
        for _ in 0..2 {
            let _ = server_stream.next().await.expect("initial frame");
        }

        let filter = |target_id: i32| {
            serde_json::json!({
                "filter": {
                    "targetId": target_id,
                    "count": 0
                }
            })
        };

        server_stream
            .send(serde_json::to_vec(&filter(1)).unwrap())
            .await
            .expect("send existence filter 1");

        runtime::sleep(std::time::Duration::from_millis(50)).await;
        wait_for_add_target(&server_stream, 1).await;

        server_stream
            .send(serde_json::to_vec(&filter(2)).unwrap())
            .await
            .expect("send existence filter 2");

        runtime::sleep(std::time::Duration::from_millis(50)).await;
        wait_for_add_target(&server_stream, 2).await;

        let events = syncer.events.lock().await.clone();
        assert!(events.iter().any(|event| event.target_resets.contains(&1)));
        assert!(events.iter().any(|event| event.target_resets.contains(&2)));
    }

    async fn wait_for_add_target(stream: &MultiplexedStream, expected_target: i32) {
        for _ in 0..8 {
            if let Some(frame) = stream.next().await {
                if let Ok(payload) = frame {
                    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&payload) {
                        if json
                            .get("addTarget")
                            .and_then(|v| v.get("targetId"))
                            .and_then(|v| v.as_i64())
                            == Some(expected_target as i64)
                        {
                            return;
                        }
                    }
                }
            }
        }
        panic!("did not observe addTarget for {expected_target}");
    }
}
