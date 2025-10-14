mod converter;
mod database;
pub mod document;
mod operations;
mod reference;
mod snapshot;

pub use converter::{FirestoreDataConverter, PassthroughConverter};
pub use database::{get_firestore, register_firestore_component, Firestore};
pub use document::FirestoreClient;
pub use operations::SetOptions;
pub use reference::{
    CollectionReference, ConvertedCollectionReference, ConvertedDocumentReference,
    DocumentReference,
};
pub use snapshot::{DocumentSnapshot, SnapshotMetadata, TypedDocumentSnapshot};
