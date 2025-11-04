pub mod connection;
pub mod datastore;
pub mod mutation;
pub mod network;
pub mod remote_event;
pub mod remote_store;
pub mod remote_syncer;
pub mod rpc_error;
pub mod serializer;
pub mod stream;
pub mod streams;
pub(crate) mod structured_query;
pub mod syncer_bridge;
pub mod watch_change;
pub mod watch_change_aggregator;

pub use connection::{Connection, ConnectionBuilder, RequestContext};
pub use datastore::{
    Datastore, HttpDatastore, InMemoryDatastore, NoopTokenProvider, RetrySettings, StreamHandle,
    StreamingDatastore, StreamingDatastoreImpl, StreamingFuture, TokenProviderArc,
};
pub use mutation::{MutationBatch, MutationBatchResult};
pub use network::{NetworkLayer, NetworkLayerBuilder, NetworkStreamHandler, StreamCredentials};
pub use remote_event::{RemoteEvent, TargetChange};
pub use remote_store::RemoteStore;
pub use remote_syncer::{box_remote_store_future, RemoteStoreFuture, RemoteSyncer};
pub use rpc_error::map_http_error;
pub use serializer::JsonProtoSerializer;
#[cfg(not(target_arch = "wasm32"))]
pub use stream::WebSocketTransport;
pub use stream::{InMemoryTransport, MultiplexedConnection, MultiplexedStream, StreamTransport};
pub use streams::{
    ListenStream, ListenStreamDelegate, ListenTarget, TargetPayload, WriteStream,
    WriteStreamDelegate,
};
pub use syncer_bridge::{RemoteSyncerBridge, RemoteSyncerDelegate};
pub use watch_change::{
    DocumentChange, DocumentDelete, DocumentRemove, ExistenceFilterChange, TargetChangeState,
    WatchChange, WatchDocument, WatchTargetChange,
};
pub use watch_change_aggregator::{TargetMetadataProvider, WatchChangeAggregator};
