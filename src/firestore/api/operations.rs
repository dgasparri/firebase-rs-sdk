use std::collections::BTreeMap;

use crate::firestore::error::FirestoreResult;
use crate::firestore::model::DocumentKey;
use crate::firestore::value::{FirestoreValue, MapValue};

use super::Firestore;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct DocumentSnapshot {
    key: DocumentKey,
    data: Option<MapValue>,
}

#[allow(dead_code)]
impl DocumentSnapshot {
    pub fn new(key: DocumentKey, data: Option<MapValue>) -> Self {
        Self { key, data }
    }

    pub fn exists(&self) -> bool {
        self.data.is_some()
    }

    pub fn data(&self) -> Option<&BTreeMap<String, FirestoreValue>> {
        self.data.as_ref().map(|map| map.fields())
    }

    pub fn id(&self) -> &str {
        self.key.id()
    }

    pub fn reference(
        &self,
        firestore: Firestore,
    ) -> FirestoreResult<super::reference::DocumentReference> {
        super::reference::DocumentReference::new(firestore, self.key.path().clone())
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
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

    #[test]
    fn snapshot_presence() {
        let key = DocumentKey::from_string("coll/doc").unwrap();
        let snapshot = DocumentSnapshot::new(key, None);
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
