mod converter;
mod database;
pub mod document;
mod operations;
pub(crate) mod query;
mod reference;
mod snapshot;

pub use converter::{FirestoreDataConverter, PassthroughConverter};
pub use database::{get_firestore, register_firestore_component, Firestore};
pub use document::FirestoreClient;
pub use operations::SetOptions;
pub use query::{ConvertedQuery, Query, QuerySnapshot, TypedQuerySnapshot};
pub use reference::{
    CollectionReference, ConvertedCollectionReference, ConvertedDocumentReference,
    DocumentReference,
};
pub use snapshot::{DocumentSnapshot, SnapshotMetadata, TypedDocumentSnapshot};
