use serde_json::{json, Value as JsonValue};

use crate::firestore::api::aggregate::{AggregateDefinition, AggregateOperation};
use crate::firestore::api::query::{Bound, FieldFilter, QueryDefinition};
use crate::firestore::error::FirestoreResult;
use crate::firestore::remote::serializer::JsonProtoSerializer;

pub(crate) fn encode_structured_query(
    serializer: &JsonProtoSerializer,
    definition: &QueryDefinition,
) -> FirestoreResult<JsonValue> {
    let mut structured = serde_json::Map::new();

    if let Some(fields) = definition.projection() {
        let field_entries: Vec<_> = fields
            .iter()
            .map(|field| json!({ "fieldPath": field.canonical_string() }))
            .collect();
        structured.insert("select".to_string(), json!({ "fields": field_entries }));
    }

    let mut from_entry = serde_json::Map::new();
    from_entry.insert(
        "collectionId".to_string(),
        json!(definition.collection_id()),
    );
    from_entry.insert(
        "allDescendants".to_string(),
        json!(definition.collection_group().is_some()),
    );
    structured.insert(
        "from".to_string(),
        JsonValue::Array(vec![JsonValue::Object(from_entry)]),
    );

    if !definition.filters().is_empty() {
        let filter_json = encode_filters(serializer, definition.filters());
        structured.insert("where".to_string(), filter_json);
    }

    if !definition.request_order_by().is_empty() {
        let orders: Vec<_> = definition
            .request_order_by()
            .iter()
            .map(|order| {
                json!({
                    "field": { "fieldPath": order.field().canonical_string() },
                    "direction": order.direction().as_str(),
                })
            })
            .collect();
        structured.insert("orderBy".to_string(), JsonValue::Array(orders));
    }

    if let Some(limit) = definition.limit() {
        structured.insert("limit".to_string(), json!(limit as i64));
    }

    if let Some(start) = definition.request_start_at() {
        structured.insert(
            "startAt".to_string(),
            encode_cursor(serializer, start, true),
        );
    }

    if let Some(end) = definition.request_end_at() {
        structured.insert("endAt".to_string(), encode_cursor(serializer, end, false));
    }

    Ok(JsonValue::Object(structured))
}

pub(crate) fn encode_aggregation_body(
    serializer: &JsonProtoSerializer,
    definition: &QueryDefinition,
    aggregations: &[AggregateDefinition],
) -> FirestoreResult<JsonValue> {
    let structured_query = encode_structured_query(serializer, definition)?;

    let mut aggregation_entries = Vec::new();
    for aggregate in aggregations {
        let mut entry = serde_json::Map::new();
        entry.insert("alias".to_string(), json!(aggregate.alias()));
        match aggregate.operation() {
            AggregateOperation::Count => {
                entry.insert("count".to_string(), json!({}));
            }
            AggregateOperation::Sum(field_path) => {
                entry.insert(
                    "sum".to_string(),
                    json!({ "field": { "fieldPath": field_path.canonical_string() } }),
                );
            }
            AggregateOperation::Average(field_path) => {
                entry.insert(
                    "avg".to_string(),
                    json!({ "field": { "fieldPath": field_path.canonical_string() } }),
                );
            }
        };
        aggregation_entries.push(JsonValue::Object(entry));
    }

    Ok(json!({
        "structuredAggregationQuery": {
            "structuredQuery": structured_query,
            "aggregations": aggregation_entries
        }
    }))
}

fn encode_filters(serializer: &JsonProtoSerializer, filters: &[FieldFilter]) -> JsonValue {
    if filters.len() == 1 {
        return encode_field_filter(serializer, &filters[0]);
    }

    let nested: Vec<_> = filters
        .iter()
        .map(|filter| encode_field_filter(serializer, filter))
        .collect();

    json!({
        "compositeFilter": {
            "op": "AND",
            "filters": nested
        }
    })
}

fn encode_field_filter(serializer: &JsonProtoSerializer, filter: &FieldFilter) -> JsonValue {
    json!({
        "fieldFilter": {
            "field": { "fieldPath": filter.field().canonical_string() },
            "op": filter.operator().as_str(),
            "value": serializer.encode_value(filter.value())
        }
    })
}

fn encode_cursor(serializer: &JsonProtoSerializer, bound: &Bound, start: bool) -> JsonValue {
    json!({
        "values": bound
            .values()
            .iter()
            .map(|value| serializer.encode_value(value))
            .collect::<Vec<_>>(),
        "before": if start { bound.inclusive() } else { !bound.inclusive() },
    })
}
