use std::collections::BTreeMap;

use crate::firestore::error::FirestoreResult;
use crate::firestore::model::DocumentKey;
use crate::firestore::value::{FirestoreValue, MapValue};

use super::reference::DocumentReference;
use super::Firestore;

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
    pub fn data(&self) -> Option<&BTreeMap<String, FirestoreValue>> {
        self.data.as_ref().map(|map| map.fields())
    }

    /// Returns snapshot metadata describing cache and mutation state.
    pub fn metadata(&self) -> &SnapshotMetadata {
        &self.metadata
    }

    /// Convenience accessor matching the JS API.
    pub fn from_cache(&self) -> bool {
        self.metadata.from_cache()
    }

    /// Convenience accessor matching the JS API.
    pub fn has_pending_writes(&self) -> bool {
        self.metadata.has_pending_writes()
    }

    pub fn id(&self) -> &str {
        self.key.id()
    }

    pub fn reference(&self, firestore: Firestore) -> FirestoreResult<DocumentReference> {
        DocumentReference::new(firestore, self.key.path().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firestore::model::DocumentKey;

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
}
