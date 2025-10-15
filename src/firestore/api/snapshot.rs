use std::collections::BTreeMap;
use std::sync::Arc;

use crate::firestore::error::FirestoreResult;
use crate::firestore::model::DocumentKey;
use crate::firestore::value::{FirestoreValue, MapValue};

use super::reference::DocumentReference;
use super::Firestore;
use super::FirestoreDataConverter;

/// Metadata about the state of a document snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SnapshotMetadata {
    from_cache: bool,
    has_pending_writes: bool,
}

impl SnapshotMetadata {
    /// Creates metadata with the provided cache/pending-write flags.
    pub fn new(from_cache: bool, has_pending_writes: bool) -> Self {
        Self {
            from_cache,
            has_pending_writes,
        }
    }

    /// Indicates whether the snapshot was served from a local cache.
    pub fn from_cache(&self) -> bool {
        self.from_cache
    }

    /// Indicates whether the snapshot contains uncommitted local mutations.
    pub fn has_pending_writes(&self) -> bool {
        self.has_pending_writes
    }
}

/// Snapshot of a document's contents.
#[derive(Clone, Debug)]
pub struct DocumentSnapshot {
    key: DocumentKey,
    data: Option<MapValue>,
    metadata: SnapshotMetadata,
}

impl DocumentSnapshot {
    pub fn new(key: DocumentKey, data: Option<MapValue>, metadata: SnapshotMetadata) -> Self {
        Self {
            key,
            data,
            metadata,
        }
    }

    /// Returns whether the document exists on the backend.
    pub fn exists(&self) -> bool {
        self.data.is_some()
    }

    /// Returns the decoded document fields if the snapshot contains data.
    ///
    /// The returned map borrows from the snapshot; mutate a clone before
    /// writing it back to Firestore.
    pub fn data(&self) -> Option<&BTreeMap<String, FirestoreValue>> {
        self.data.as_ref().map(|map| map.fields())
    }

    /// Returns snapshot metadata describing cache and mutation state.
    pub fn metadata(&self) -> &SnapshotMetadata {
        &self.metadata
    }

    /// Returns the underlying map value for advanced conversions.
    pub fn map_value(&self) -> Option<&MapValue> {
        self.data.as_ref()
    }

    /// Convenience accessor matching the JS API.
    pub fn from_cache(&self) -> bool {
        self.metadata.from_cache()
    }

    /// Convenience accessor matching the JS API.
    pub fn has_pending_writes(&self) -> bool {
        self.metadata.has_pending_writes()
    }

    /// Returns the identifier of the document represented by this snapshot.
    pub fn id(&self) -> &str {
        self.key.id()
    }

    pub(crate) fn document_key(&self) -> &DocumentKey {
        &self.key
    }

    /// Creates a document reference pointing at the same location as this snapshot.
    pub fn reference(&self, firestore: Firestore) -> FirestoreResult<DocumentReference> {
        DocumentReference::new(firestore, self.key.path().clone())
    }
    /// Converts this snapshot into a typed snapshot using the provided converter.
    pub fn into_typed<C>(self, converter: Arc<C>) -> TypedDocumentSnapshot<C>
    where
        C: FirestoreDataConverter,
    {
        TypedDocumentSnapshot::new(self, converter)
    }

    /// Returns a typed snapshot by cloning the underlying data and converter.
    pub fn to_typed<C>(&self, converter: Arc<C>) -> TypedDocumentSnapshot<C>
    where
        C: FirestoreDataConverter,
    {
        self.clone().into_typed(converter)
    }
}

/// Document snapshot carrying a converter for typed access.
#[derive(Clone)]
pub struct TypedDocumentSnapshot<C>
where
    C: FirestoreDataConverter,
{
    base: DocumentSnapshot,
    converter: Arc<C>,
}

impl<C> TypedDocumentSnapshot<C>
where
    C: FirestoreDataConverter,
{
    pub fn new(base: DocumentSnapshot, converter: Arc<C>) -> Self {
        Self { base, converter }
    }

    pub fn exists(&self) -> bool {
        self.base.exists()
    }

    pub fn id(&self) -> &str {
        self.base.id()
    }

    pub fn metadata(&self) -> &SnapshotMetadata {
        self.base.metadata()
    }

    pub fn from_cache(&self) -> bool {
        self.base.from_cache()
    }

    pub fn has_pending_writes(&self) -> bool {
        self.base.has_pending_writes()
    }

    pub fn reference(&self, firestore: Firestore) -> FirestoreResult<DocumentReference> {
        self.base.reference(firestore)
    }

    pub fn raw(&self) -> &DocumentSnapshot {
        &self.base
    }

    pub fn into_raw(self) -> DocumentSnapshot {
        self.base
    }

    /// Returns the typed model using the embedded converter.
    pub fn data(&self) -> FirestoreResult<Option<C::Model>> {
        match self.base.map_value() {
            Some(map) => self.converter.from_map(map).map(Some),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firestore::api::converter::{FirestoreDataConverter, PassthroughConverter};
    use crate::firestore::model::DocumentKey;
    use crate::firestore::value::ValueKind;
    use std::collections::BTreeMap;

    #[test]
    fn metadata_flags() {
        let meta = SnapshotMetadata::new(true, false);
        assert!(meta.from_cache());
        assert!(!meta.has_pending_writes());
    }

    #[test]
    fn snapshot_reports_existence() {
        let key = DocumentKey::from_string("cities/sf").unwrap();
        let snapshot = DocumentSnapshot::new(key, None, SnapshotMetadata::default());
        assert!(!snapshot.exists());
    }

    #[derive(Clone)]
    struct NameConverter;

    impl FirestoreDataConverter for NameConverter {
        type Model = String;

        fn to_map(&self, value: &Self::Model) -> FirestoreResult<BTreeMap<String, FirestoreValue>> {
            let mut map = BTreeMap::new();
            map.insert("name".to_string(), FirestoreValue::from_string(value));
            Ok(map)
        }

        fn from_map(&self, value: &MapValue) -> FirestoreResult<Self::Model> {
            match value.fields().get("name").and_then(|val| match val.kind() {
                ValueKind::String(s) => Some(s.clone()),
                _ => None,
            }) {
                Some(name) => Ok(name),
                None => Err(crate::firestore::error::invalid_argument(
                    "missing name field",
                )),
            }
        }
    }

    #[test]
    fn typed_snapshot_uses_converter() {
        let key = DocumentKey::from_string("cities/sf").unwrap();
        let mut map = BTreeMap::new();
        map.insert(
            "name".to_string(),
            FirestoreValue::from_string("San Francisco"),
        );
        let snapshot = DocumentSnapshot::new(
            key,
            Some(MapValue::new(map)),
            SnapshotMetadata::new(false, false),
        );

        let typed = snapshot.into_typed(Arc::new(NameConverter));
        let name = typed.data().unwrap();
        assert_eq!(name.as_deref(), Some("San Francisco"));
    }

    #[test]
    fn passthrough_converter_roundtrip() {
        let key = DocumentKey::from_string("cities/sf").unwrap();
        let mut map = BTreeMap::new();
        map.insert("name".to_string(), FirestoreValue::from_string("SF"));
        let snapshot = DocumentSnapshot::new(
            key,
            Some(MapValue::new(map.clone())),
            SnapshotMetadata::default(),
        );

        let typed = snapshot.into_typed(Arc::new(PassthroughConverter::default()));
        let raw = typed.data().unwrap().unwrap();
        assert_eq!(raw.get("name"), map.get("name"));
    }
}
