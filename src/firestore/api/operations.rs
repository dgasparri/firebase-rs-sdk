use std::collections::BTreeMap;

use crate::firestore::error::FirestoreResult;
use crate::firestore::model::DocumentKey;
use crate::firestore::value::{FirestoreValue, MapValue};

#[derive(Clone, Debug)]
pub struct SetOptions {
    pub merge: bool,
}

impl Default for SetOptions {
    fn default() -> Self {
        Self { merge: false }
    }
}

#[allow(dead_code)]
pub fn encode_document_data(data: BTreeMap<String, FirestoreValue>) -> FirestoreResult<MapValue> {
    Ok(MapValue::new(data))
}

#[allow(dead_code)]
pub fn validate_document_path(path: &str) -> FirestoreResult<DocumentKey> {
    let key = DocumentKey::from_string(path)?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firestore::api::snapshot::{DocumentSnapshot, SnapshotMetadata};

    #[test]
    fn snapshot_presence() {
        let key = DocumentKey::from_string("coll/doc").unwrap();
        let snapshot = DocumentSnapshot::new(key, None, SnapshotMetadata::default());
        assert!(!snapshot.exists());
    }

    #[test]
    fn map_encodes() {
        let mut data = BTreeMap::new();
        data.insert("name".to_string(), FirestoreValue::from_string("Ada"));
        let map = encode_document_data(data).unwrap();
        assert!(map.fields().contains_key("name"));
    }
}
