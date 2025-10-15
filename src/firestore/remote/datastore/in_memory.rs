use std::collections::BTreeMap;

use super::Datastore;
use crate::firestore::api::query::QueryDefinition;
use crate::firestore::api::{DocumentSnapshot, SnapshotMetadata};
use crate::firestore::error::FirestoreResult;
use crate::firestore::model::DocumentKey;
use crate::firestore::value::MapValue;

#[derive(Clone, Default)]
pub struct InMemoryDatastore {
    documents: std::sync::Arc<std::sync::Mutex<BTreeMap<String, MapValue>>>,
}

impl InMemoryDatastore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Datastore for InMemoryDatastore {
    fn get_document(&self, key: &DocumentKey) -> FirestoreResult<DocumentSnapshot> {
        let store = self.documents.lock().unwrap();
        let data = store.get(&key.path().canonical_string()).cloned();
        Ok(DocumentSnapshot::new(
            key.clone(),
            data,
            SnapshotMetadata::new(true, false),
        ))
    }

    fn set_document(&self, key: &DocumentKey, data: MapValue, _merge: bool) -> FirestoreResult<()> {
        let mut store = self.documents.lock().unwrap();
        store.insert(key.path().canonical_string(), data);
        Ok(())
    }

    fn run_query(&self, query: &QueryDefinition) -> FirestoreResult<Vec<DocumentSnapshot>> {
        let store = self.documents.lock().unwrap();
        let mut results = Vec::new();
        for (path, data) in store.iter() {
            let key = DocumentKey::from_string(path)?;
            if query.matches(&key) {
                results.push((
                    path.clone(),
                    DocumentSnapshot::new(
                        key,
                        Some(data.clone()),
                        SnapshotMetadata::new(true, false),
                    ),
                ));
            }
        }
        results.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(results.into_iter().map(|(_, snapshot)| snapshot).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firestore::value::FirestoreValue;

    #[test]
    fn in_memory_get_set() {
        let datastore = InMemoryDatastore::new();
        let key = DocumentKey::from_string("cities/sf").unwrap();
        let mut map = BTreeMap::new();
        map.insert("name".to_string(), FirestoreValue::from_string("SF"));
        let map = MapValue::new(map);
        datastore.set_document(&key, map.clone(), false).unwrap();
        let snapshot = datastore.get_document(&key).unwrap();
        assert!(snapshot.exists());
        assert_eq!(
            snapshot.data().unwrap().get("name"),
            Some(&FirestoreValue::from_string("SF"))
        );
    }
}
