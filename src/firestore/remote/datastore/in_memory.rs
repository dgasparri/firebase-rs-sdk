use std::collections::BTreeMap;

use crate::firestore::api::query::{
    Bound, FieldFilter, FilterOperator, OrderBy, OrderDirection, QueryDefinition,
};
use crate::firestore::api::{DocumentSnapshot, SnapshotMetadata};
use crate::firestore::error::{internal_error, not_found, FirestoreResult};
use crate::firestore::model::{DocumentKey, FieldPath};
use crate::firestore::value::{FirestoreValue, MapValue, ValueKind};

use async_trait::async_trait;

use super::Datastore;

#[derive(Clone, Default)]
pub struct InMemoryDatastore {
    documents: std::sync::Arc<std::sync::Mutex<BTreeMap<String, MapValue>>>,
}

impl InMemoryDatastore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Datastore for InMemoryDatastore {
    async fn get_document(&self, key: &DocumentKey) -> FirestoreResult<DocumentSnapshot> {
        let store = self.documents.lock().unwrap();
        let data = store.get(&key.path().canonical_string()).cloned();
        Ok(DocumentSnapshot::new(
            key.clone(),
            data,
            SnapshotMetadata::new(true, false),
        ))
    }

    async fn set_document(
        &self,
        key: &DocumentKey,
        data: MapValue,
        _merge: bool,
    ) -> FirestoreResult<()> {
        let mut store = self.documents.lock().unwrap();
        store.insert(key.path().canonical_string(), data);
        Ok(())
    }

    async fn run_query(&self, query: &QueryDefinition) -> FirestoreResult<Vec<DocumentSnapshot>> {
        let store = self.documents.lock().unwrap();
        let mut documents = Vec::new();

        for (path, data) in store.iter() {
            let key = DocumentKey::from_string(path)?;
            if !query.matches_collection(&key) {
                continue;
            }

            let snapshot =
                DocumentSnapshot::new(key, Some(data.clone()), SnapshotMetadata::new(true, false));

            if document_satisfies_filters(&snapshot, query.filters()) {
                documents.push(snapshot);
            }
        }

        documents.sort_by(|left, right| compare_snapshots(left, right, query.result_order_by()));

        if let Some(bound) = query.result_start_at() {
            documents.retain(|snapshot| {
                !is_before_start_bound(snapshot, bound, query.result_order_by())
            });
        }

        if let Some(bound) = query.result_end_at() {
            documents
                .retain(|snapshot| !is_after_end_bound(snapshot, bound, query.result_order_by()));
        }

        if let Some(limit) = query.limit() {
            let limit = limit as usize;
            match query.limit_type() {
                crate::firestore::api::query::LimitType::First => {
                    if documents.len() > limit {
                        documents.truncate(limit);
                    }
                }
                crate::firestore::api::query::LimitType::Last => {
                    if documents.len() > limit {
                        let start = documents.len() - limit;
                        documents.drain(0..start);
                    }
                }
            }
        }

        Ok(documents)
    }

    async fn update_document(
        &self,
        key: &DocumentKey,
        data: MapValue,
        field_paths: Vec<FieldPath>,
    ) -> FirestoreResult<()> {
        let mut store = self.documents.lock().unwrap();
        let canonical = key.path().canonical_string();
        let current = store
            .get(&canonical)
            .cloned()
            .ok_or_else(|| not_found(format!("Document {} does not exist", canonical)))?;

        let mut fields = current.fields().clone();
        for path in &field_paths {
            let value = value_for_path(&data, path.segments()).ok_or_else(|| {
                internal_error(format!(
                    "Failed to resolve value for update path {}",
                    path.canonical_string()
                ))
            })?;
            set_value_at_path(&mut fields, path.segments(), value);
        }

        store.insert(canonical, MapValue::new(fields));
        Ok(())
    }

    async fn delete_document(&self, key: &DocumentKey) -> FirestoreResult<()> {
        let mut store = self.documents.lock().unwrap();
        store.remove(&key.path().canonical_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firestore::value::FirestoreValue;

    #[tokio::test]
    async fn in_memory_get_set() {
        let datastore = InMemoryDatastore::new();
        let key = DocumentKey::from_string("cities/sf").unwrap();
        let mut map = BTreeMap::new();
        map.insert("name".to_string(), FirestoreValue::from_string("SF"));
        let map = MapValue::new(map);
        datastore
            .set_document(&key, map.clone(), false)
            .await
            .unwrap();
        let snapshot = datastore.get_document(&key).await.unwrap();
        assert!(snapshot.exists());
        assert_eq!(
            snapshot.data().unwrap().get("name"),
            Some(&FirestoreValue::from_string("SF"))
        );
    }
}

fn document_satisfies_filters(snapshot: &DocumentSnapshot, filters: &[FieldFilter]) -> bool {
    filters
        .iter()
        .all(|filter| match get_field_value(snapshot, filter.field()) {
            Some(value) => evaluate_filter(filter, &value),
            None => {
                filter.operator() == FilterOperator::NotEqual
                    && evaluate_filter(filter, &FirestoreValue::null())
            }
        })
}

fn evaluate_filter(filter: &FieldFilter, value: &FirestoreValue) -> bool {
    match filter.operator() {
        FilterOperator::Equal => value == filter.value(),
        FilterOperator::NotEqual => value != filter.value(),
        FilterOperator::LessThan => {
            compare_values(value, filter.value()) == Some(std::cmp::Ordering::Less)
        }
        FilterOperator::LessThanOrEqual => match compare_values(value, filter.value()) {
            Some(std::cmp::Ordering::Less) | Some(std::cmp::Ordering::Equal) => true,
            _ => false,
        },
        FilterOperator::GreaterThan => {
            compare_values(value, filter.value()) == Some(std::cmp::Ordering::Greater)
        }
        FilterOperator::GreaterThanOrEqual => match compare_values(value, filter.value()) {
            Some(std::cmp::Ordering::Greater) | Some(std::cmp::Ordering::Equal) => true,
            _ => false,
        },
        FilterOperator::NotIn
        | FilterOperator::ArrayContains
        | FilterOperator::ArrayContainsAny
        | FilterOperator::In => false,
    }
}

fn get_field_value(snapshot: &DocumentSnapshot, field: &FieldPath) -> Option<FirestoreValue> {
    if field == &FieldPath::document_id() {
        let key = snapshot.document_key();
        return Some(FirestoreValue::from_string(key.path().canonical_string()));
    }

    let map = snapshot.map_value()?;
    find_in_map(map, field.segments()).cloned()
}

fn find_in_map<'a>(map: &'a MapValue, segments: &'a [String]) -> Option<&'a FirestoreValue> {
    let (first, rest) = segments.split_first()?;
    let value = map.fields().get(first)?;
    if rest.is_empty() {
        Some(value)
    } else if let ValueKind::Map(child) = value.kind() {
        find_in_map(child, rest)
    } else {
        None
    }
}

fn compare_snapshots(
    left: &DocumentSnapshot,
    right: &DocumentSnapshot,
    order_by: &[OrderBy],
) -> std::cmp::Ordering {
    for order in order_by {
        let left_value = get_field_value(left, order.field()).unwrap_or_else(FirestoreValue::null);
        let right_value =
            get_field_value(right, order.field()).unwrap_or_else(FirestoreValue::null);

        let mut ordering =
            compare_values(&left_value, &right_value).unwrap_or(std::cmp::Ordering::Equal);
        if order.direction() == OrderDirection::Descending {
            ordering = ordering.reverse();
        }
        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
    }
    std::cmp::Ordering::Equal
}

fn value_for_path(map: &MapValue, segments: &[String]) -> Option<FirestoreValue> {
    let (first, rest) = segments.split_first()?;
    let value = map.fields().get(first)?;
    if rest.is_empty() {
        return Some(value.clone());
    }
    match value.kind() {
        ValueKind::Map(child) => value_for_path(child, rest),
        _ => None,
    }
}

fn set_value_at_path(
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

    set_value_at_path(&mut child_fields, &segments[1..], value);
    *entry = FirestoreValue::from_map(child_fields);
}

fn compare_values(left: &FirestoreValue, right: &FirestoreValue) -> Option<std::cmp::Ordering> {
    match (left.kind(), right.kind()) {
        (ValueKind::Null, ValueKind::Null) => Some(std::cmp::Ordering::Equal),
        (ValueKind::Boolean(a), ValueKind::Boolean(b)) => Some(a.cmp(b)),
        (ValueKind::Integer(a), ValueKind::Integer(b)) => Some(a.cmp(b)),
        (ValueKind::Double(a), ValueKind::Double(b)) => a.partial_cmp(b),
        (ValueKind::Integer(a), ValueKind::Double(b)) => (*a as f64).partial_cmp(b),
        (ValueKind::Double(a), ValueKind::Integer(b)) => a.partial_cmp(&(*b as f64)),
        (ValueKind::String(a), ValueKind::String(b)) => Some(a.cmp(b)),
        (ValueKind::Reference(a), ValueKind::Reference(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

fn is_before_start_bound(snapshot: &DocumentSnapshot, bound: &Bound, order_by: &[OrderBy]) -> bool {
    let ordering = compare_snapshot_to_bound(snapshot, bound, order_by);
    if bound.inclusive() {
        ordering == std::cmp::Ordering::Less
    } else {
        ordering != std::cmp::Ordering::Greater
    }
}

fn is_after_end_bound(snapshot: &DocumentSnapshot, bound: &Bound, order_by: &[OrderBy]) -> bool {
    let ordering = compare_snapshot_to_bound(snapshot, bound, order_by);
    if bound.inclusive() {
        ordering == std::cmp::Ordering::Greater
    } else {
        ordering != std::cmp::Ordering::Less
    }
}

fn compare_snapshot_to_bound(
    snapshot: &DocumentSnapshot,
    bound: &Bound,
    order_by: &[OrderBy],
) -> std::cmp::Ordering {
    for (index, order) in order_by.iter().enumerate() {
        if index >= bound.values().len() {
            break;
        }

        let bound_value = &bound.values()[index];
        let snapshot_value =
            get_field_value(snapshot, order.field()).unwrap_or_else(FirestoreValue::null);

        let mut ordering =
            compare_values(&snapshot_value, bound_value).unwrap_or(std::cmp::Ordering::Equal);
        if order.direction() == OrderDirection::Descending {
            ordering = ordering.reverse();
        }

        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
    }
    std::cmp::Ordering::Equal
}
