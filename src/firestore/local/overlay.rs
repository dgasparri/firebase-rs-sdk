use std::collections::BTreeMap;

use crate::firestore::api::operations::{
    set_value_at_field_path, value_for_field_path, FieldTransform, TransformOperation,
};
use crate::firestore::error::{invalid_argument, FirestoreResult};
use crate::firestore::model::{FieldPath, Timestamp};
use crate::firestore::remote::datastore::WriteOperation;
use crate::firestore::value::{FirestoreValue, MapValue, ValueKind};

/// Applies a sequence of overlay writes to the provided document, mirroring the
/// latency-compensated view used by the JS SDK.
pub(crate) fn apply_document_overlays(
    base: Option<MapValue>,
    overlays: &[WriteOperation],
) -> FirestoreResult<Option<MapValue>> {
    let mut current = base;
    for write in overlays {
        current = apply_overlay_once(current, write)?;
    }
    Ok(current)
}

fn apply_overlay_once(
    current: Option<MapValue>,
    write: &WriteOperation,
) -> FirestoreResult<Option<MapValue>> {
    match write {
        WriteOperation::Set {
            data,
            mask,
            transforms,
            ..
        } => apply_set_overlay(current, data, mask.as_deref(), transforms),
        WriteOperation::Update {
            data,
            field_paths,
            transforms,
            ..
        } => apply_update_overlay(current, data, field_paths, transforms),
        WriteOperation::Delete { .. } => Ok(None),
    }
}

fn apply_set_overlay(
    current: Option<MapValue>,
    data: &MapValue,
    mask: Option<&[FieldPath]>,
    transforms: &[FieldTransform],
) -> FirestoreResult<Option<MapValue>> {
    let mut fields = match mask {
        Some(mask) => {
            let mut fields = current
                .as_ref()
                .map(|map| map.fields().clone())
                .unwrap_or_default();
            for path in mask {
                if let Some(value) = value_for_field_path(data, path) {
                    set_value_at_field_path(&mut fields, path, value);
                } else {
                    remove_value_at_field_path(&mut fields, path);
                }
            }
            fields
        }
        None => data.fields().clone(),
    };

    apply_field_transforms(&mut fields, transforms)?;
    Ok(Some(MapValue::new(fields)))
}

fn apply_update_overlay(
    current: Option<MapValue>,
    data: &MapValue,
    field_paths: &[FieldPath],
    transforms: &[FieldTransform],
) -> FirestoreResult<Option<MapValue>> {
    let mut fields = current
        .as_ref()
        .map(|map| map.fields().clone())
        .unwrap_or_default();

    for path in field_paths {
        let value = value_for_field_path(data, path).ok_or_else(|| {
            invalid_argument(format!(
                "Failed to resolve value for update path {}",
                path.canonical_string()
            ))
        })?;
        set_value_at_field_path(&mut fields, path, value);
    }

    apply_field_transforms(&mut fields, transforms)?;
    Ok(Some(MapValue::new(fields)))
}

fn apply_field_transforms(
    fields: &mut BTreeMap<String, FirestoreValue>,
    transforms: &[FieldTransform],
) -> FirestoreResult<()> {
    if transforms.is_empty() {
        return Ok(());
    }

    let base_map = MapValue::new(fields.clone());
    for transform in transforms {
        let path = transform.field_path();
        let current_value = value_for_field_path(&base_map, path);
        let new_value = match transform.operation() {
            TransformOperation::ServerTimestamp => FirestoreValue::from_timestamp(Timestamp::now()),
            TransformOperation::ArrayUnion(elements) => array_union(current_value, elements),
            TransformOperation::ArrayRemove(elements) => array_remove(current_value, elements),
            TransformOperation::NumericIncrement(operand) => {
                numeric_increment(current_value, operand)?
            }
        };
        set_value_at_field_path(fields, path, new_value);
    }

    Ok(())
}

fn array_union(existing: Option<FirestoreValue>, additions: &[FirestoreValue]) -> FirestoreValue {
    let mut values = match existing {
        Some(value) => match value.kind() {
            ValueKind::Array(array) => array.values().to_vec(),
            _ => Vec::new(),
        },
        None => Vec::new(),
    };

    for element in additions {
        if !values.iter().any(|candidate| candidate == element) {
            values.push(element.clone());
        }
    }

    FirestoreValue::from_array(values)
}

fn array_remove(existing: Option<FirestoreValue>, removals: &[FirestoreValue]) -> FirestoreValue {
    let values = match existing {
        Some(value) => match value.kind() {
            ValueKind::Array(array) => array.values().to_vec(),
            _ => Vec::new(),
        },
        None => Vec::new(),
    };

    let filtered: Vec<FirestoreValue> = values
        .into_iter()
        .filter(|candidate| !removals.iter().any(|needle| needle == candidate))
        .collect();

    FirestoreValue::from_array(filtered)
}

fn numeric_increment(
    existing: Option<FirestoreValue>,
    operand: &FirestoreValue,
) -> FirestoreResult<FirestoreValue> {
    let result = match (existing, operand.kind()) {
        (Some(value), ValueKind::Integer(delta)) => match value.kind() {
            ValueKind::Integer(current) => {
                if let Some(sum) = current.checked_add(*delta) {
                    FirestoreValue::from_integer(sum)
                } else {
                    FirestoreValue::from_double(*current as f64 + *delta as f64)
                }
            }
            ValueKind::Double(current) => FirestoreValue::from_double(*current + *delta as f64),
            _ => FirestoreValue::from_integer(*delta),
        },
        (Some(value), ValueKind::Double(delta)) => match value.kind() {
            ValueKind::Integer(current) => FirestoreValue::from_double(*current as f64 + *delta),
            ValueKind::Double(current) => FirestoreValue::from_double(*current + *delta),
            _ => FirestoreValue::from_double(*delta),
        },
        (None, ValueKind::Integer(delta)) => FirestoreValue::from_integer(*delta),
        (None, ValueKind::Double(delta)) => FirestoreValue::from_double(*delta),
        (_, _) => {
            return Err(invalid_argument(
                "FieldValue.increment() requires a numeric operand",
            ))
        }
    };

    Ok(result)
}

fn remove_value_at_field_path(fields: &mut BTreeMap<String, FirestoreValue>, path: &FieldPath) {
    remove_value_at_segments(fields, path.segments());
}

fn remove_value_at_segments(fields: &mut BTreeMap<String, FirestoreValue>, segments: &[String]) {
    if segments.is_empty() {
        return;
    }

    if segments.len() == 1 {
        fields.remove(&segments[0]);
        return;
    }

    let first = &segments[0];
    if let Some(value) = fields.get(first).cloned() {
        if let ValueKind::Map(child_map) = value.kind() {
            let mut child_fields = child_map.fields().clone();
            remove_value_at_segments(&mut child_fields, &segments[1..]);
            if child_fields.is_empty() {
                fields.remove(first);
            } else {
                fields.insert(first.clone(), FirestoreValue::from_map(child_fields));
            }
        }
    }
}
