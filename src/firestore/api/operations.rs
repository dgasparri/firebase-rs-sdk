use std::collections::{BTreeMap, HashSet};

use crate::firestore::error::{invalid_argument, FirestoreResult};
use crate::firestore::model::{DocumentKey, FieldPath};
use crate::firestore::value::{FirestoreValue, MapValue, ValueKind};

/// Options that configure the behaviour of `set_doc`/`set_doc_with_converter` writes.
///
/// Mirrors the modular JS `SetOptions` type from
/// `packages/firestore/src/lite-api/reference_impl.ts`, including `merge` and
/// `mergeFields` support.
#[derive(Clone, Debug, Default)]
pub struct SetOptions {
    /// When `true`, `set_doc` merges the provided data into the existing
    /// document, matching the JS `merge: true` option.
    pub merge: bool,
    /// Explicit field mask that should be merged. When set, this takes
    /// precedence over the `merge` flag.
    pub merge_fields: Option<Vec<FieldPath>>,
}

impl SetOptions {
    /// Builds set options that merge every field present in the provided data.
    ///
    /// TypeScript reference: `SetOptions` (`merge: true`) in
    /// `packages/firestore/src/lite-api/reference_impl.ts`.
    pub fn merge_all() -> Self {
        Self {
            merge: true,
            merge_fields: None,
        }
    }

    /// Builds set options that merge only the specified field paths.
    ///
    /// TypeScript reference: `SetOptions` (`mergeFields`) in
    /// `packages/firestore/src/lite-api/reference_impl.ts`.
    pub fn merge_fields<I>(fields: I) -> FirestoreResult<Self>
    where
        I: IntoIterator<Item = FieldPath>,
    {
        let mut unique = Vec::new();
        let mut seen = HashSet::new();
        for field in fields {
            if seen.insert(field.canonical_string()) {
                unique.push(field);
            }
        }
        if unique.is_empty() {
            return Err(invalid_argument(
                "merge_fields requires at least one field path",
            ));
        }
        Ok(Self {
            merge: false,
            merge_fields: Some(unique),
        })
    }

    /// Indicates whether the write should behave like a merge.
    pub fn is_merge(&self) -> bool {
        self.merge || self.merge_fields.is_some()
    }

    /// Returns the explicit field mask, if any.
    pub fn field_mask(&self) -> Option<&[FieldPath]> {
        self.merge_fields.as_deref()
    }
}

#[allow(dead_code)]
pub fn encode_document_data(data: BTreeMap<String, FirestoreValue>) -> FirestoreResult<MapValue> {
    Ok(MapValue::new(data))
}

pub fn encode_set_data(
    data: BTreeMap<String, FirestoreValue>,
    options: &SetOptions,
) -> FirestoreResult<(MapValue, Option<Vec<FieldPath>>)> {
    let map = MapValue::new(data);

    if let Some(mask) = options.field_mask() {
        validate_mask_against_map(&map, mask)?;
        return Ok((map, Some(mask.to_vec())));
    }

    if options.merge {
        let field_paths = collect_update_paths(map.fields())?;
        if field_paths.is_empty() {
            return Err(invalid_argument(
                "merge set requires the data to contain at least one field",
            ));
        }
        return Ok((map, Some(field_paths)));
    }

    Ok((map, None))
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

pub(crate) fn value_for_field_path(map: &MapValue, path: &FieldPath) -> Option<FirestoreValue> {
    value_for_segments(map, path.segments())
}

pub(crate) fn value_for_segments(map: &MapValue, segments: &[String]) -> Option<FirestoreValue> {
    let (first, rest) = segments.split_first()?;
    let value = map.fields().get(first)?;
    if rest.is_empty() {
        Some(value.clone())
    } else if let ValueKind::Map(child) = value.kind() {
        value_for_segments(child, rest)
    } else {
        None
    }
}

pub(crate) fn set_value_at_field_path(
    fields: &mut BTreeMap<String, FirestoreValue>,
    path: &FieldPath,
    value: FirestoreValue,
) {
    set_value_at_segments(fields, path.segments(), value);
}

fn set_value_at_segments(
    fields: &mut BTreeMap<String, FirestoreValue>,
    segments: &[String],
    value: FirestoreValue,
) {
    if segments.is_empty() {
        return;
    }

    if segments.len() == 1 {
        fields.insert(segments[0].clone(), value);
        return;
    }

    let first = &segments[0];
    let entry = fields
        .entry(first.clone())
        .or_insert_with(|| FirestoreValue::from_map(BTreeMap::new()));

    let mut child_fields = match entry.kind() {
        ValueKind::Map(map) => map.fields().clone(),
        _ => BTreeMap::new(),
    };

    set_value_at_segments(&mut child_fields, &segments[1..], value);
    *entry = FirestoreValue::from_map(child_fields);
}

fn validate_mask_against_map(map: &MapValue, mask: &[FieldPath]) -> FirestoreResult<()> {
    if mask.is_empty() {
        return Err(invalid_argument(
            "merge_fields requires at least one field path",
        ));
    }

    for field in mask {
        if value_for_field_path(map, field).is_none() {
            return Err(invalid_argument(format!(
                "Field '{}' is specified in merge_fields but missing from the provided data",
                field.canonical_string()
            )));
        }
    }
    Ok(())
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
    fn merge_collects_field_paths() {
        let mut data = BTreeMap::new();
        let mut stats = BTreeMap::new();
        stats.insert("visits".to_string(), FirestoreValue::from_integer(42));
        data.insert("stats".to_string(), FirestoreValue::from_map(stats));
        let options = SetOptions::merge_all();
        let (_map, mask) = encode_set_data(data, &options).unwrap();
        let mask = mask.expect("mask");
        assert_eq!(mask.len(), 1);
        assert_eq!(mask[0].canonical_string(), "stats.visits");
    }

    #[test]
    fn merge_fields_validate_presence() {
        let mut data = BTreeMap::new();
        data.insert("display".to_string(), FirestoreValue::from_string("Ada"));
        let options =
            SetOptions::merge_fields(vec![FieldPath::from_dot_separated("display").unwrap()])
                .unwrap();
        let result = encode_set_data(data.clone(), &options);
        assert!(result.is_ok());

        let missing =
            SetOptions::merge_fields(vec![FieldPath::from_dot_separated("missing").unwrap()])
                .unwrap();
        let err = encode_set_data(data, &missing).unwrap_err();
        assert_eq!(err.code_str(), "firestore/invalid-argument");
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
