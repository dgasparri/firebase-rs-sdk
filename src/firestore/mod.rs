#![doc = include_str!("README.md")]
pub mod api;
mod constants;
pub mod error;
pub mod model;
pub mod remote;
pub mod value;

#[doc(inline)]
pub use api::{
    get_firestore, register_firestore_component, CollectionReference, ConvertedCollectionReference,
    ConvertedDocumentReference, ConvertedQuery, DocumentReference, DocumentSnapshot,
    FilterOperator, Firestore, FirestoreClient, FirestoreDataConverter, LimitType, OrderDirection,
    PassthroughConverter, Query, QuerySnapshot, SetOptions, SnapshotMetadata,
    TypedDocumentSnapshot, TypedQuerySnapshot,
};

#[doc(inline)]
pub use api::query::QueryDefinition;

#[doc(inline)]
pub use constants::{DEFAULT_DATABASE_ID, FIRESTORE_COMPONENT_NAME};

#[doc(inline)]
pub use model::{DatabaseId, DocumentKey, FieldPath, GeoPoint, ResourcePath, Timestamp};

#[doc(inline)]
pub use remote::{
    map_http_error, Connection, ConnectionBuilder, Datastore, HttpDatastore, InMemoryDatastore,
    JsonProtoSerializer, NoopTokenProvider, RequestContext, RetrySettings, TokenProviderArc,
};

#[doc(inline)]
pub use remote::datastore::{http::HttpDatastoreBuilder, TokenProvider};

#[doc(inline)]
pub use value::{ArrayValue, BytesValue, FirestoreValue, MapValue, ValueKind};

#[doc(inline)]
pub use error::{FirestoreError, FirestoreErrorCode, FirestoreResult};
