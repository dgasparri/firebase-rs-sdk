#![doc = include_str!("README.md")]
pub mod api;
mod constants;
pub mod error;
pub mod model;
pub mod remote;
pub mod value;

#[doc(inline)]
pub use api::{
    get_firestore, register_firestore_component, AggregateField, AggregateQuerySnapshot,
    AggregateSpec, CollectionReference, ConvertedCollectionReference, ConvertedDocumentReference,
    ConvertedQuery, DocumentReference, DocumentSnapshot, FilterOperator, Firestore,
    FirestoreClient, FirestoreDataConverter, LimitType, OrderDirection, PassthroughConverter,
    Query, QuerySnapshot, SetOptions, SnapshotMetadata, TypedDocumentSnapshot, TypedQuerySnapshot,
    WriteBatch,
};

#[doc(inline)]
pub use api::query::QueryDefinition;

#[doc(inline)]
pub use constants::{DEFAULT_DATABASE_ID, FIRESTORE_COMPONENT_NAME};

#[doc(inline)]
pub use model::{DatabaseId, DocumentKey, FieldPath, GeoPoint, ResourcePath, Timestamp};

#[cfg(not(target_arch = "wasm32"))]
pub use remote::WebSocketTransport;
#[doc(inline)]
pub use remote::{
    box_remote_store_future, map_http_error, Connection, ConnectionBuilder, Datastore,
    ExistenceFilterChange, HttpDatastore, InMemoryDatastore, InMemoryTransport,
    JsonProtoSerializer, ListenStream, ListenStreamDelegate, ListenTarget, MultiplexedConnection,
    MultiplexedStream, MutationBatch, MutationBatchResult, NetworkLayer, NetworkLayerBuilder,
    NetworkStreamHandler, NoopTokenProvider, RemoteEvent, RemoteStore, RemoteStoreFuture,
    RemoteSyncer, RemoteSyncerBridge, RemoteSyncerDelegate, RequestContext, RetrySettings,
    StreamCredentials, StreamHandle, StreamTransport, StreamingDatastore, StreamingDatastoreImpl,
    StreamingFuture, TargetChange, TargetChangeState, TargetMetadataProvider, TargetPayload,
    TokenProviderArc, WatchChange, WatchChangeAggregator, WatchDocument, WatchTargetChange,
};

#[doc(inline)]
pub use remote::streams::{WriteResponse, WriteResult, WriteStream, WriteStreamDelegate};

#[doc(inline)]
pub use remote::datastore::{http::HttpDatastoreBuilder, TokenProvider};

#[doc(inline)]
pub use value::{ArrayValue, BytesValue, FirestoreValue, MapValue, ValueKind};

#[doc(inline)]
pub use error::{FirestoreError, FirestoreErrorCode, FirestoreResult};
