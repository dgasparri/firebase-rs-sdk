use std::collections::BTreeMap;

use crate::firestore::error::FirestoreResult;
use crate::firestore::value::{FirestoreValue, MapValue};

/// Trait describing how to convert between user models and Firestore maps.
///
/// This mirrors the modular JS `FirestoreDataConverter` contract: writes use
/// `to_map`, reads use `from_map`, and callers choose the `Model` type they
/// want to surface.
pub trait FirestoreDataConverter: Send + Sync + Clone + 'static {
    /// The strongly typed model associated with this converter.
    type Model: Clone;

    /// Encodes the user model into a Firestore map for writes.
    fn to_map(&self, value: &Self::Model) -> FirestoreResult<BTreeMap<String, FirestoreValue>>;

    /// Decodes a Firestore map into the user model for reads.
    fn from_map(&self, value: &MapValue) -> FirestoreResult<Self::Model>;
}

/// Default converter that leaves Firestore maps unchanged (raw JSON-style data).
#[derive(Clone, Default)]
pub struct PassthroughConverter;

impl FirestoreDataConverter for PassthroughConverter {
    type Model = BTreeMap<String, FirestoreValue>;

    fn to_map(&self, value: &Self::Model) -> FirestoreResult<BTreeMap<String, FirestoreValue>> {
        Ok(value.clone())
    }

    fn from_map(&self, value: &MapValue) -> FirestoreResult<Self::Model> {
        Ok(value.fields().clone())
    }
}
