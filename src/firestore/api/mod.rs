pub(crate) mod aggregate;
mod converter;
mod database;
pub mod document;
pub(crate) mod operations;
pub(crate) mod query;
mod reference;
mod snapshot;
mod write_batch;

pub use aggregate::{AggregateField, AggregateQuerySnapshot, AggregateSpec};
pub use converter::{FirestoreDataConverter, PassthroughConverter};
pub use database::{get_firestore, register_firestore_component, Firestore};
pub use document::FirestoreClient;
pub use operations::SetOptions;
pub use query::{
    ConvertedQuery, FilterOperator, LimitType, OrderDirection, Query, QuerySnapshot,
    TypedQuerySnapshot,
};
pub use reference::{
    CollectionReference, ConvertedCollectionReference, ConvertedDocumentReference,
    DocumentReference,
};
pub use snapshot::{DocumentSnapshot, SnapshotMetadata, TypedDocumentSnapshot};
pub use write_batch::WriteBatch;
