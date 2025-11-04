use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::firestore::api::{Query, QuerySnapshot};
use crate::firestore::error::FirestoreResult;
use crate::firestore::local::memory::{
    MemoryLocalStore, QueryListenerRegistration, TargetMetadataSnapshot,
};
use crate::firestore::model::DocumentKey;
use crate::firestore::remote::syncer_bridge::RemoteSyncerBridge;
use crate::firestore::remote::{
    JsonProtoSerializer, ListenTarget, NetworkLayer, RemoteStore, RemoteSyncer,
};

/// Coordinates the local and remote stores, mirroring the responsibilities of the
/// Firestore JS SyncEngine.
pub struct SyncEngine {
    local_store: Arc<MemoryLocalStore>,
    remote_store: RemoteStore,
    remote_bridge: Arc<RemoteSyncerBridge<MemoryLocalStore>>,
}

impl SyncEngine {
    /// Creates a sync engine using the provided `MemoryLocalStore` (with optional
    /// persistence) and remote networking stack.
    pub fn new(
        local_store: Arc<MemoryLocalStore>,
        network_layer: NetworkLayer,
        serializer: JsonProtoSerializer,
    ) -> Self {
        let bridge = Arc::new(RemoteSyncerBridge::new(Arc::clone(&local_store)));
        local_store.synchronize_remote_keys(&bridge);

        let remote_store = RemoteStore::new(
            network_layer,
            serializer,
            Arc::clone(&bridge) as Arc<dyn RemoteSyncer>,
        );

        Self {
            local_store,
            remote_store,
            remote_bridge: bridge,
        }
    }

    /// Constructs a sync engine backed by IndexedDB persistence when running on
    /// wasm targets.
    #[cfg(all(
        feature = "wasm-web",
        feature = "experimental-indexed-db",
        target_arch = "wasm32"
    ))]
    pub fn with_indexed_db(
        db_name: impl Into<String>,
        network_layer: NetworkLayer,
        serializer: JsonProtoSerializer,
    ) -> Self {
        let local_store = MemoryLocalStore::new_with_indexed_db(db_name);
        Self::new(local_store, network_layer, serializer)
    }

    pub fn local_store(&self) -> &Arc<MemoryLocalStore> {
        &self.local_store
    }

    pub fn remote_store(&self) -> &RemoteStore {
        &self.remote_store
    }

    pub fn remote_bridge(&self) -> Arc<RemoteSyncerBridge<MemoryLocalStore>> {
        Arc::clone(&self.remote_bridge)
    }

    pub fn target_metadata(&self) -> BTreeMap<i32, TargetMetadataSnapshot> {
        self.local_store.target_metadata_map()
    }

    pub fn remote_keys_for_target(&self, target_id: i32) -> BTreeSet<DocumentKey> {
        self.remote_bridge.remote_keys_for_target(target_id)
    }

    pub async fn listen_query<F>(
        &self,
        target: ListenTarget,
        query: Query,
        callback: F,
    ) -> FirestoreResult<QueryListenerRegistration>
    where
        F: Fn(QuerySnapshot) + Send + Sync + 'static,
    {
        let target_id = target.target_id();
        let callback_arc: Arc<dyn Fn(QuerySnapshot) + Send + Sync> = Arc::new(callback);
        let mut registration = self
            .local_store
            .register_query_listener(target_id, query, callback_arc)
            .await?;

        if let Err(err) = self.remote_store.listen(target).await {
            registration.detach();
            return Err(err);
        }

        Ok(registration)
    }

    pub async fn listen(&self, target: ListenTarget) -> FirestoreResult<()> {
        self.remote_store.listen(target).await
    }

    pub async fn unlisten(&self, target_id: i32) -> FirestoreResult<()> {
        self.remote_store.unlisten(target_id).await
    }

    pub async fn unlisten_query(
        &self,
        target_id: i32,
        registration: &mut QueryListenerRegistration,
    ) -> FirestoreResult<()> {
        registration.detach();
        self.remote_store.unlisten(target_id).await
    }

    pub async fn enable_network(&self) -> FirestoreResult<()> {
        self.remote_store.enable_network().await
    }

    pub async fn disable_network(&self) -> FirestoreResult<()> {
        self.remote_store.disable_network().await
    }

    pub async fn pump_writes(&self) -> FirestoreResult<()> {
        self.remote_store.pump_writes().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firestore::api::Firestore;
    use crate::firestore::model::{DatabaseId, DocumentKey, ResourcePath};
    use crate::firestore::remote::remote_event::{RemoteEvent, TargetChange};
    use crate::firestore::remote::watch_change::WatchDocument;
    use crate::firestore::remote::NoopTokenProvider;
    use crate::firestore::remote::{
        InMemoryTransport, JsonProtoSerializer, MultiplexedConnection, NetworkLayer,
        StreamingDatastore, StreamingDatastoreImpl, TokenProviderArc,
    };
    use crate::firestore::value::MapValue;
    use crate::test_support::firebase::test_firebase_app_with_api_key;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    fn sample_network() -> (
        NetworkLayer,
        JsonProtoSerializer,
        Arc<MultiplexedConnection>,
    ) {
        let (client_transport, server_transport) = InMemoryTransport::pair();
        let client_connection = Arc::new(MultiplexedConnection::new(client_transport));
        let server_connection = Arc::new(MultiplexedConnection::new(server_transport));
        let datastore = StreamingDatastoreImpl::new(Arc::clone(&client_connection));
        let datastore: Arc<dyn StreamingDatastore> = Arc::new(datastore);
        let token_provider: TokenProviderArc = Arc::new(NoopTokenProvider::default());
        let network = NetworkLayer::builder(datastore, token_provider).build();
        let serializer = JsonProtoSerializer::new(DatabaseId::new("test", "(default)"));
        (network, serializer, server_connection)
    }

    fn build_query() -> Query {
        let app = test_firebase_app_with_api_key("sync-engine-query");
        let firestore = Firestore::new(app, DatabaseId::new("test", "(default)"));
        let path = ResourcePath::from_string("cities").expect("collection path");
        Query::new(firestore, path).expect("query")
    }

    #[tokio::test]
    async fn seeds_remote_keys_from_restored_metadata() {
        let store = Arc::new(MemoryLocalStore::new());
        let key = DocumentKey::from_string("cities/sf").unwrap();
        let mut snapshot = TargetMetadataSnapshot::new(1);
        snapshot.remote_keys.insert(key.clone());
        store.restore_target_snapshot(snapshot);

        let (network, serializer, _server_connection) = sample_network();
        let engine = SyncEngine::new(Arc::clone(&store), network, serializer);
        let remote_keys = engine.remote_bridge.remote_keys_for_target(1);
        assert!(remote_keys.contains(&key));
    }

    #[tokio::test]
    async fn query_listener_receives_remote_updates() {
        let store = Arc::new(MemoryLocalStore::new());
        let (network, serializer, _server_connection) = sample_network();
        let engine = SyncEngine::new(Arc::clone(&store), network, serializer.clone());

        let query = build_query();
        let definition = query.definition();
        let target = ListenTarget::for_query(&serializer, 1, &definition).expect("listen target");

        let records = Arc::new(Mutex::new(Vec::new()));
        let callback_records = Arc::clone(&records);
        let mut registration = engine
            .listen_query(target.clone(), query.clone(), move |snapshot| {
                callback_records.lock().unwrap().push(snapshot.len());
            })
            .await
            .expect("listen");

        let key = DocumentKey::from_string("cities/sf").unwrap();
        let mut change = TargetChange::default();
        change.added_documents.insert(key.clone());
        let watch_doc = WatchDocument {
            key: key.clone(),
            fields: MapValue::new(BTreeMap::new()),
            update_time: None,
            create_time: None,
        };

        let mut event = RemoteEvent::default();
        event.target_changes.insert(1, change);
        event.document_updates.insert(key.clone(), Some(watch_doc));

        engine
            .remote_bridge
            .apply_remote_event(event)
            .await
            .expect("apply event");

        let snapshot_counts = records.lock().unwrap().clone();
        assert_eq!(snapshot_counts, vec![0, 1]);

        engine
            .unlisten_query(1, &mut registration)
            .await
            .expect("unlisten");
    }
}
