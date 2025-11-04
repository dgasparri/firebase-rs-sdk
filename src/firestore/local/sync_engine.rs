use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::firestore::error::FirestoreResult;
use crate::firestore::local::memory::{MemoryLocalStore, TargetMetadataSnapshot};
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

    pub async fn listen(&self, target: ListenTarget) -> FirestoreResult<()> {
        self.remote_store.listen(target).await
    }

    pub async fn unlisten(&self, target_id: i32) -> FirestoreResult<()> {
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
    use crate::firestore::model::{DatabaseId, DocumentKey};
    use crate::firestore::remote::NoopTokenProvider;
    use crate::firestore::remote::{
        InMemoryTransport, JsonProtoSerializer, MultiplexedConnection, NetworkLayer,
        StreamingDatastore, StreamingDatastoreImpl, TokenProviderArc,
    };

    fn sample_network() -> (NetworkLayer, JsonProtoSerializer) {
        let (client_transport, _server_transport) = InMemoryTransport::pair();
        let client_connection = Arc::new(MultiplexedConnection::new(client_transport));
        let datastore = StreamingDatastoreImpl::new(Arc::clone(&client_connection));
        let datastore: Arc<dyn StreamingDatastore> = Arc::new(datastore);
        let token_provider: TokenProviderArc = Arc::new(NoopTokenProvider::default());
        let network = NetworkLayer::builder(datastore, token_provider).build();
        let serializer = JsonProtoSerializer::new(DatabaseId::new("test", "(default)"));
        (network, serializer)
    }

    #[tokio::test]
    async fn seeds_remote_keys_from_restored_metadata() {
        let store = Arc::new(MemoryLocalStore::new());
        let key = DocumentKey::from_string("cities/sf")
            .unwrap();
        let mut snapshot = TargetMetadataSnapshot::new(1);
        snapshot.remote_keys.insert(key.clone());
        store.restore_target_snapshot(snapshot);

        let (network, serializer) = sample_network();
        let engine = SyncEngine::new(Arc::clone(&store), network, serializer);
        let remote_keys = engine.remote_bridge.remote_keys_for_target(1);
        assert!(remote_keys.contains(&key));
    }
}
