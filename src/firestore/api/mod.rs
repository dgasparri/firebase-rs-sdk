mod aggregate;
mod converter;
mod database;
mod document;
mod operations;
mod query;
mod reference;
mod snapshot;
mod write_batch;

pub(crate) use aggregate::AggregateOperation;

#[doc(inline)]
pub use aggregate::{AggregateDefinition, AggregateField, AggregateQuerySnapshot, AggregateSpec};

#[doc(inline)]
pub use converter::{FirestoreDataConverter, PassthroughConverter};

#[doc(inline)]
pub use database::{get_firestore, register_firestore_component, Firestore};

#[doc(inline)]
pub use document::FirestoreClient;

#[doc(inline)]
pub use operations::{
    encode_document_data, encode_set_data, encode_update_document_data, validate_document_path,
    EncodedSetData, EncodedUpdateData, FieldTransform, SetOptions, TransformOperation,
};

pub(crate) use operations::{set_value_at_field_path, value_for_field_path};

#[doc(inline)]
pub use query::{
    ConvertedQuery, DocumentChangeType, FilterOperator, LimitType, OrderDirection, Query,
    QueryDocumentChange, QuerySnapshot, QuerySnapshotMetadata, TypedQueryDocumentChange,
    TypedQuerySnapshot,
};

pub(crate) use query::{compute_doc_changes, Bound, FieldFilter, OrderBy, QueryDefinition};

#[doc(inline)]
pub use reference::{
    CollectionReference, ConvertedCollectionReference, ConvertedDocumentReference,
    DocumentReference,
};

#[doc(inline)]
pub use snapshot::{DocumentSnapshot, SnapshotMetadata, TypedDocumentSnapshot};

#[doc(inline)]
pub use write_batch::WriteBatch;
