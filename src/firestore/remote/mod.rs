pub mod connection;
pub mod datastore;
pub mod network;
pub mod rpc_error;
pub mod serializer;
pub mod stream;
pub mod streams;
pub(crate) mod structured_query;

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
    DocumentChange, DocumentDelete, DocumentRemove, ExistenceFilter, ListenErrorCause,
    ListenResponse, ListenStream, ListenStreamDelegate, ListenTarget, ListenTargetChange,
    TargetChangeState, TargetPayload, WriteResponse, WriteResult, WriteStream, WriteStreamDelegate,
};
