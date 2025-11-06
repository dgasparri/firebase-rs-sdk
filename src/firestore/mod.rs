#![doc = include_str!("README.md")]
mod api; 
mod constants;
mod error;
mod local;
mod model;
mod query_evaluator;
mod remote;
mod value;


pub(crate) use api::aggregate::AggregateOperation;

#[doc(inline)]
pub use api::aggregate::{AggregateDefinition, AggregateField, AggregateQuerySnapshot, AggregateSpec};

#[doc(inline)]
pub use api::converter::{FirestoreDataConverter, PassthroughConverter};

#[doc(inline)]
pub use api::database::{get_firestore, register_firestore_component, Firestore};

#[doc(inline)]
pub use api::document::FirestoreClient;

#[doc(inline)]
pub use api::operations::{
    encode_document_data, encode_set_data, encode_update_document_data, validate_document_path,
    EncodedSetData, EncodedUpdateData, FieldTransform, SetOptions, TransformOperation,
};

pub(crate) use api::operations::{set_value_at_field_path, value_for_field_path};

#[doc(inline)]
pub use api::query::{
    ConvertedQuery, DocumentChangeType, FilterOperator, LimitType, OrderDirection, Query,
    QueryDocumentChange, QuerySnapshot, QuerySnapshotMetadata, TypedQueryDocumentChange,
    TypedQuerySnapshot,
};


#[allow(unused_imports)]
pub(crate) use api::query::{compute_doc_changes, Bound, FieldFilter, OrderBy, QueryDefinition};

#[doc(inline)]
pub use api::reference::{
    CollectionReference, ConvertedCollectionReference, ConvertedDocumentReference,
    DocumentReference,
};

#[doc(inline)]
pub use api::snapshot::{DocumentSnapshot, SnapshotMetadata, TypedDocumentSnapshot};

#[doc(inline)]
pub use api::write_batch::WriteBatch;

#[doc(inline)]
pub use constants::{DEFAULT_DATABASE_ID, FIRESTORE_COMPONENT_NAME};

#[doc(inline)]
pub use error::{
    invalid_argument, missing_project_id, internal_error, not_found, permission_denied, unauthenticated,
    unavailable, deadline_exceeded, resource_exhausted,
    FirestoreError, FirestoreErrorCode, FirestoreResult
};


#[doc(inline)]
pub use local::memory::{
    LocalStorePersistence, MemoryLocalStore, PersistedQueryViewState, QueryListenerRegistration, TargetMetadataSnapshot,
};

#[allow(unused_imports)]
pub(crate) use local::overlay::apply_document_overlays;

#[doc(inline)]
pub use local::sync_engine::SyncEngine;

#[doc(inline)]
pub use model::{ 
    DatabaseId,
    DocumentKey,
    FieldPath,
    IntoFieldPath,
    GeoPoint,
    ResourcePath,
    Timestamp,
};


#[allow(unused_imports)]
pub(crate) use query_evaluator::apply_query_to_documents;


#[doc(inline)]
pub use remote::connection::{Connection, ConnectionBuilder, RequestContext};

#[doc(inline)]
pub use remote::network::{NetworkLayer, NetworkLayerBuilder, NetworkStreamHandler, StreamCredentials};

#[doc(inline)]
pub use remote::datastore::{
    Datastore, HttpDatastore, HttpDatastoreBuilder, InMemoryDatastore, NoopTokenProvider, RetrySettings, StreamHandle,
    StreamingDatastore, StreamingDatastoreImpl, StreamingHandleImpl, StreamingFuture, TokenProvider, TokenProviderArc, WriteOperation,
};

#[allow(unused_imports)]
pub(crate) use remote::datastore::box_stream_future;

#[doc(inline)]
pub use remote::mutation::{MutationBatch, MutationBatchResult};

#[doc(inline)]
pub use remote::remote_event::{RemoteEvent, TargetChange};

#[doc(inline)]
pub use remote::remote_store::RemoteStore;

#[doc(inline)]
pub use remote::remote_syncer::{box_remote_store_future, RemoteStoreFuture, RemoteSyncer};

#[doc(inline)]
pub use remote::rpc_error::map_http_error;

#[doc(inline)]
pub use remote::serializer::JsonProtoSerializer;

#[cfg(not(target_arch = "wasm32"))]
#[doc(inline)]
pub use remote::stream::WebSocketTransport;

#[doc(inline)]
pub use remote::stream::{
    InMemoryTransport, MultiplexedConnection, MultiplexedConnectionHandle, MultiplexedStream, StreamTransport,
    StreamId, FrameKind, TransportFrame,
};

#[doc(inline)]
pub use remote::stream::persistent::{
    PersistentStream, PersistentStreamDelegate, PersistentStreamHandle, StreamKind,
};

#[doc(inline)]
pub use remote::streams::listen::{ListenStream, ListenStreamDelegate, ListenTarget, TargetPayload};

#[doc(inline)]
pub use remote::streams::write::{WriteResponse, WriteResult, WriteStream, WriteStreamDelegate};

#[allow(unused_imports)]
pub(crate) use remote::structured_query::{encode_structured_query, encode_aggregation_body};

#[doc(inline)]
pub use remote::syncer_bridge::{RemoteSyncerBridge, RemoteSyncerDelegate, TargetMetadataUpdate};

#[doc(inline)]
pub use remote::watch_change::{
    decode_watch_change, DocumentChange, DocumentDelete, DocumentRemove, ExistenceFilterChange, TargetChangeState,
    WatchChange, WatchDocument, WatchTargetChange,
};

#[doc(inline)]
pub use remote::watch_change_aggregator::{TargetMetadataProvider, WatchChangeAggregator};

#[doc(inline)]
pub use value::{ArrayValue, BytesValue, FirestoreValue, MapValue, SentinelValue, ValueKind};
