#![doc = include_str!("README.md")]
mod api;
mod constants;
pub mod error;
pub mod local;
pub mod model;
pub(crate) mod query_evaluator;
pub mod remote;
pub mod value;

#[doc(inline)]
pub use api::{
    encode_document_data, encode_set_data, encode_update_document_data, get_firestore,
    register_firestore_component, validate_document_path, AggregateDefinition, AggregateField,
    AggregateQuerySnapshot, AggregateSpec, CollectionReference, ConvertedCollectionReference,
    ConvertedDocumentReference, ConvertedQuery, DocumentChangeType, DocumentReference,
    DocumentSnapshot, EncodedSetData, EncodedUpdateData, FieldTransform, FilterOperator, Firestore,
    FirestoreClient, FirestoreDataConverter, LimitType, OrderDirection, PassthroughConverter,
    Query, QueryDocumentChange, QuerySnapshot, QuerySnapshotMetadata, SetOptions, SnapshotMetadata,
    TransformOperation, TypedDocumentSnapshot, TypedQueryDocumentChange, TypedQuerySnapshot,
    WriteBatch,
};

#[allow(unused_imports)]
pub(crate) use api::{
    compute_doc_changes, set_value_at_field_path, value_for_field_path, AggregateOperation, Bound,
    FieldFilter, OrderBy, QueryDefinition,
};

#[doc(inline)]
pub use constants::{DEFAULT_DATABASE_ID, FIRESTORE_COMPONENT_NAME};

#[doc(inline)]
pub use model::{DatabaseId, DocumentKey, FieldPath, GeoPoint, ResourcePath, Timestamp};

#[doc(inline)]
pub use local::{
    LocalStorePersistence, MemoryLocalStore, QueryListenerRegistration, SyncEngine,
    TargetMetadataSnapshot,
};

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
