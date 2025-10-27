use std::collections::BTreeMap;

use crate::firestore::error::{invalid_argument, FirestoreResult};
use crate::firestore::model::{DocumentKey, FieldPath};
use crate::firestore::value::{FirestoreValue, MapValue, ValueKind};

/// Options that configure the behaviour of `set_doc` writes.
#[derive(Clone, Debug, Default)]
pub struct SetOptions {
    /// When `true`, `set_doc` behaves like the JS `merge: true` option.
    ///
    /// Merging is not yet implemented for the HTTP datastore.
    pub merge: bool,
}

#[allow(dead_code)]
pub fn encode_document_data(data: BTreeMap<String, FirestoreValue>) -> FirestoreResult<MapValue> {
    Ok(MapValue::new(data))
}

pub fn encode_update_document_data(
    data: BTreeMap<String, FirestoreValue>,
) -> FirestoreResult<(MapValue, Vec<FieldPath>)> {
    if data.is_empty() {
        return Err(invalid_argument(
            "update_doc requires at least one field/value pair",
        ));
    }
    let field_paths = collect_update_paths(&data)?;
    let map = MapValue::new(data);
    Ok((map, field_paths))
}

#[allow(dead_code)]
pub fn validate_document_path(path: &str) -> FirestoreResult<DocumentKey> {
    let key = DocumentKey::from_string(path)?;
    Ok(key)
}

fn collect_update_paths(
    data: &BTreeMap<String, FirestoreValue>,
) -> FirestoreResult<Vec<FieldPath>> {
    let mut paths = Vec::new();
    for (key, value) in data {
        let mut segments = Vec::new();
        segments.push(key.clone());
        collect_paths_from_value(&mut paths, segments, value)?;
    }
    Ok(paths)
}

fn collect_paths_from_value(
    acc: &mut Vec<FieldPath>,
    segments: Vec<String>,
    value: &FirestoreValue,
) -> FirestoreResult<()> {
    match value.kind() {
        ValueKind::Map(map) if !map.fields().is_empty() => {
            for (child_key, child_value) in map.fields() {
                let mut child_segments = segments.clone();
                child_segments.push(child_key.clone());
                collect_paths_from_value(acc, child_segments, child_value)?;
            }
            Ok(())
        }
        _ => {
            acc.push(FieldPath::new(segments)?);
            Ok(())
        }
    }
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

    #[test]
    fn update_paths_include_nested_fields() {
        let mut data = BTreeMap::new();
        data.insert("name".to_string(), FirestoreValue::from_string("Ada"));
        let mut stats = BTreeMap::new();
        stats.insert("visits".to_string(), FirestoreValue::from_integer(42));
        data.insert("stats".to_string(), FirestoreValue::from_map(stats));

        let (_map, paths) = encode_update_document_data(data).unwrap();
        let mut mask: Vec<String> = paths
            .into_iter()
            .map(|path| path.canonical_string())
            .collect();
        mask.sort();
        assert_eq!(mask, vec!["name", "stats.visits"]);
    }

    #[test]
    fn update_requires_fields() {
        let err = encode_update_document_data(BTreeMap::new()).unwrap_err();
        assert_eq!(err.code_str(), "firestore/invalid-argument");
    }
}
