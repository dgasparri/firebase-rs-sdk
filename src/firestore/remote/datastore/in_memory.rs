use std::collections::BTreeMap;

use crate::firestore::api::aggregate::{AggregateDefinition, AggregateOperation};
use crate::firestore::api::operations::{
    set_value_at_field_path, value_for_field_path, FieldTransform, TransformOperation,
};
use crate::firestore::api::query::QueryDefinition;
use crate::firestore::api::{DocumentSnapshot, SnapshotMetadata};
use crate::firestore::error::{internal_error, invalid_argument, not_found, FirestoreResult};
use crate::firestore::model::{DocumentKey, FieldPath, Timestamp};
use crate::firestore::query_evaluator::apply_query_to_documents;
use crate::firestore::value::{FirestoreValue, MapValue, ValueKind};

use async_trait::async_trait;

use super::{Datastore, WriteOperation};

#[derive(Clone, Default)]
pub struct InMemoryDatastore {
    documents: std::sync::Arc<std::sync::Mutex<BTreeMap<String, MapValue>>>,
}

impl InMemoryDatastore {
    pub fn new() -> Self {
        Self::default()
    }

    fn apply_set(
        &self,
        key: DocumentKey,
        data: MapValue,
        mask: Option<Vec<FieldPath>>,
        transforms: Vec<FieldTransform>,
    ) -> FirestoreResult<()> {
        let mut store = self.documents.lock().unwrap();
        let canonical = key.path().canonical_string();

        let mut fields = match mask {
            Some(mask) => {
                let mut fields = store
                    .get(&canonical)
                    .map(|existing| existing.fields().clone())
                    .unwrap_or_default();
                for field in mask {
                    if let Some(value) = value_for_field_path(&data, &field) {
                        set_value_at_field_path(&mut fields, &field, value);
                    }
                }
                fields
            }
            None => data.fields().clone(),
        };

        apply_field_transforms(&mut fields, &transforms)?;

        store.insert(canonical, MapValue::new(fields));
        Ok(())
    }

    fn apply_update(
        &self,
        key: DocumentKey,
        data: MapValue,
        field_paths: Vec<FieldPath>,
        transforms: Vec<FieldTransform>,
    ) -> FirestoreResult<()> {
        let mut store = self.documents.lock().unwrap();
        let canonical = key.path().canonical_string();
        let current = store
            .get(&canonical)
            .cloned()
            .ok_or_else(|| not_found(format!("Document {} does not exist", canonical)))?;

        let mut fields = current.fields().clone();
        for path in &field_paths {
            let value = value_for_field_path(&data, path).ok_or_else(|| {
                internal_error(format!(
                    "Failed to resolve value for update path {}",
                    path.canonical_string()
                ))
            })?;
            set_value_at_field_path(&mut fields, path, value);
        }

        apply_field_transforms(&mut fields, &transforms)?;

        store.insert(canonical, MapValue::new(fields));
        Ok(())
    }

    fn apply_delete(&self, key: DocumentKey) -> FirestoreResult<()> {
        let mut store = self.documents.lock().unwrap();
        store.remove(&key.path().canonical_string());
        Ok(())
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
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
        mask: Option<Vec<FieldPath>>,
        transforms: Vec<FieldTransform>,
    ) -> FirestoreResult<()> {
        self.apply_set(key.clone(), data, mask, transforms)
    }

    async fn run_query(&self, query: &QueryDefinition) -> FirestoreResult<Vec<DocumentSnapshot>> {
        let store = self.documents.lock().unwrap();
        let mut documents = Vec::new();

        for (path, data) in store.iter() {
            let key = DocumentKey::from_string(path)?;
            if !query.matches_collection(&key) {
                continue;
            }

            documents.push(DocumentSnapshot::new(
                key,
                Some(data.clone()),
                SnapshotMetadata::new(true, false),
            ));
        }

        Ok(apply_query_to_documents(documents, query))
    }

    async fn update_document(
        &self,
        key: &DocumentKey,
        data: MapValue,
        field_paths: Vec<FieldPath>,
        transforms: Vec<FieldTransform>,
    ) -> FirestoreResult<()> {
        self.apply_update(key.clone(), data, field_paths, transforms)
    }

    async fn delete_document(&self, key: &DocumentKey) -> FirestoreResult<()> {
        self.apply_delete(key.clone())
    }

    async fn commit(&self, writes: Vec<WriteOperation>) -> FirestoreResult<()> {
        for write in writes {
            match write {
                WriteOperation::Set {
                    key,
                    data,
                    mask,
                    transforms,
                } => {
                    self.apply_set(key, data, mask, transforms)?;
                }
                WriteOperation::Update {
                    key,
                    data,
                    field_paths,
                    transforms,
                } => {
                    self.apply_update(key, data, field_paths, transforms)?;
                }
                WriteOperation::Delete { key } => {
                    self.apply_delete(key)?;
                }
            }
        }
        Ok(())
    }

    async fn run_aggregate(
        &self,
        query: &QueryDefinition,
        aggregations: &[AggregateDefinition],
    ) -> FirestoreResult<BTreeMap<String, FirestoreValue>> {
        let documents = self.run_query(query).await?;
        let mut results = BTreeMap::new();

        for aggregate in aggregations {
            let value = match aggregate.operation() {
                AggregateOperation::Count => FirestoreValue::from_integer(documents.len() as i64),
                AggregateOperation::Sum(field_path) => sum_numeric(&documents, field_path),
                AggregateOperation::Average(field_path) => average_numeric(&documents, field_path),
            };
            results.insert(aggregate.alias().to_string(), value);
        }

        Ok(results)
    }
}

fn sum_numeric(documents: &[DocumentSnapshot], field_path: &FieldPath) -> FirestoreValue {
    let mut total = 0f64;
    let mut has_double = false;
    let mut has_value = false;

    for snapshot in documents {
        if let Some(map) = snapshot.map_value() {
            if let Some(value) = map.get(field_path) {
                match value.kind() {
                    ValueKind::Integer(i) => {
                        total += *i as f64;
                        has_value = true;
                    }
                    ValueKind::Double(d) => {
                        total += *d;
                        has_value = true;
                        has_double = true;
                    }
                    _ => {}
                }
            }
        }
    }

    if !has_value {
        FirestoreValue::from_integer(0)
    } else if has_double || (total.fract() != 0.0) {
        FirestoreValue::from_double(total)
    } else {
        FirestoreValue::from_integer(total as i64)
    }
}

fn average_numeric(documents: &[DocumentSnapshot], field_path: &FieldPath) -> FirestoreValue {
    let mut total = 0f64;
    let mut count = 0usize;

    for snapshot in documents {
        if let Some(map) = snapshot.map_value() {
            if let Some(value) = map.get(field_path) {
                match value.kind() {
                    ValueKind::Integer(i) => {
                        total += *i as f64;
                        count += 1;
                    }
                    ValueKind::Double(d) => {
                        total += *d;
                        count += 1;
                    }
                    _ => {}
                }
            }
        }
    }

    if count == 0 {
        FirestoreValue::null()
    } else {
        FirestoreValue::from_double(total / count as f64)
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
            .set_document(&key, map.clone(), None, Vec::new())
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

fn apply_field_transforms(
    fields: &mut BTreeMap<String, FirestoreValue>,
    transforms: &[FieldTransform],
) -> FirestoreResult<()> {
    if transforms.is_empty() {
        return Ok(());
    }

    for transform in transforms {
        let path = transform.field_path();
        let current_value = value_for_field_path(&MapValue::new(fields.clone()), path);
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
