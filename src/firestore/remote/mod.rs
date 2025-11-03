pub mod connection;
pub mod datastore;
pub mod network;
pub mod rpc_error;
pub mod serializer;
pub mod stream;
pub mod streams;
pub(crate) mod structured_query;
pub mod watch_change;

pub use connection::{Connection, ConnectionBuilder, RequestContext};
pub use datastore::{
    Datastore, HttpDatastore, InMemoryDatastore, NoopTokenProvider, RetrySettings, StreamHandle,
    StreamingDatastore, StreamingDatastoreImpl, StreamingFuture, TokenProviderArc,
};
pub use network::{NetworkLayer, NetworkLayerBuilder, NetworkStreamHandler, StreamCredentials};
pub use rpc_error::map_http_error;
pub use serializer::JsonProtoSerializer;
#[cfg(not(target_arch = "wasm32"))]
pub use stream::WebSocketTransport;
pub use stream::{InMemoryTransport, MultiplexedConnection, MultiplexedStream, StreamTransport};
pub use streams::{
    ListenStream, ListenStreamDelegate, ListenTarget, TargetPayload, WriteStream,
    WriteStreamDelegate,
};
pub use watch_change::{
    DocumentChange, DocumentDelete, DocumentRemove, ExistenceFilterChange, TargetChangeState,
    WatchChange, WatchDocument, WatchTargetChange,
};
