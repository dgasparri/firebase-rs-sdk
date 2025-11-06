use std::cmp::Ordering;

use crate::firestore::{
    Bound, FieldFilter, FilterOperator, LimitType, OrderBy, OrderDirection, QueryDefinition,
};
use crate::firestore::api::DocumentSnapshot;
use crate::firestore::model::FieldPath;
use crate::firestore::value::{FirestoreValue, MapValue, ValueKind};

/// Applies the provided query definition to a set of candidate documents and returns
/// the filtered, ordered, and bounded result set.
///
/// Mirrors the behaviour of the Firestore JS query evaluation helpers found in
/// `packages/firestore/src/core/query.ts` and related files by reusing the same
/// ordering, cursor, and limit semantics.
pub(crate) fn apply_query_to_documents(
    documents: Vec<DocumentSnapshot>,
    definition: &QueryDefinition,
) -> Vec<DocumentSnapshot> {
    let mut filtered: Vec<DocumentSnapshot> = documents
        .into_iter()
        .filter(|snapshot| snapshot.exists())
        .filter(|snapshot| document_satisfies_filters(snapshot, definition.filters()))
        .collect();

    filtered.sort_by(|left, right| compare_snapshots(left, right, definition.result_order_by()));

    if let Some(bound) = definition.result_start_at() {
        filtered.retain(|snapshot| {
            !is_before_start_bound(snapshot, bound, definition.result_order_by())
        });
    }

    if let Some(bound) = definition.result_end_at() {
        filtered
            .retain(|snapshot| !is_after_end_bound(snapshot, bound, definition.result_order_by()));
    }

    if let Some(limit) = definition.limit() {
        let limit = limit as usize;
        match definition.limit_type() {
            LimitType::First => {
                if filtered.len() > limit {
                    filtered.truncate(limit);
                }
            }
            LimitType::Last => {
                if filtered.len() > limit {
                    let start = filtered.len() - limit;
                    filtered.drain(0..start);
                }
            }
        }
    }

    filtered
}

fn document_satisfies_filters(snapshot: &DocumentSnapshot, filters: &[FieldFilter]) -> bool {
    filters
        .iter()
        .all(|filter| match get_field_value(snapshot, filter.field()) {
            Some(value) => evaluate_filter(filter, &value),
            None => match filter.operator() {
                FilterOperator::NotEqual => evaluate_filter(filter, &FirestoreValue::null()),
                FilterOperator::NotIn => false,
                _ => false,
            },
        })
}

fn evaluate_filter(filter: &FieldFilter, value: &FirestoreValue) -> bool {
    match filter.operator() {
        FilterOperator::Equal => value == filter.value(),
        FilterOperator::NotEqual => value != filter.value(),
        FilterOperator::LessThan => compare_values(value, filter.value()) == Some(Ordering::Less),
        FilterOperator::LessThanOrEqual => matches!(
            compare_values(value, filter.value()),
            Some(Ordering::Less | Ordering::Equal)
        ),
        FilterOperator::GreaterThan => {
            compare_values(value, filter.value()) == Some(Ordering::Greater)
        }
        FilterOperator::GreaterThanOrEqual => matches!(
            compare_values(value, filter.value()),
            Some(Ordering::Greater | Ordering::Equal)
        ),
        FilterOperator::ArrayContains => match value.kind() {
            ValueKind::Array(array) => array_contains(array, filter.value()),
            _ => false,
        },
        FilterOperator::ArrayContainsAny => match (value.kind(), filter.value().kind()) {
            (ValueKind::Array(array), ValueKind::Array(needles)) => {
                array_contains_any(array, needles)
            }
            _ => false,
        },
        FilterOperator::In => match filter.value().kind() {
            ValueKind::Array(values) => values.values().iter().any(|needle| needle == value),
            _ => false,
        },
        FilterOperator::NotIn => match filter.value().kind() {
            ValueKind::Array(values) => {
                !matches!(value.kind(), ValueKind::Null)
                    && values.values().iter().all(|needle| needle != value)
            }
            _ => false,
        },
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
) -> Ordering {
    for order in order_by {
        let left_value = get_field_value(left, order.field()).unwrap_or_else(FirestoreValue::null);
        let right_value =
            get_field_value(right, order.field()).unwrap_or_else(FirestoreValue::null);

        let mut ordering = compare_values(&left_value, &right_value).unwrap_or(Ordering::Equal);
        if order.direction() == OrderDirection::Descending {
            ordering = ordering.reverse();
        }
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    Ordering::Equal
}

fn compare_values(left: &FirestoreValue, right: &FirestoreValue) -> Option<Ordering> {
    match (left.kind(), right.kind()) {
        (ValueKind::Null, ValueKind::Null) => Some(Ordering::Equal),
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

fn array_contains(array: &crate::firestore::value::ArrayValue, needle: &FirestoreValue) -> bool {
    array.values().iter().any(|candidate| candidate == needle)
}

fn array_contains_any(
    array: &crate::firestore::value::ArrayValue,
    needles: &crate::firestore::value::ArrayValue,
) -> bool {
    needles
        .values()
        .iter()
        .any(|needle| array_contains(array, needle))
}

fn is_before_start_bound(snapshot: &DocumentSnapshot, bound: &Bound, order_by: &[OrderBy]) -> bool {
    let ordering = compare_snapshot_to_bound(snapshot, bound, order_by);
    if bound.inclusive() {
        ordering == Ordering::Less
    } else {
        ordering != Ordering::Greater
    }
}

fn is_after_end_bound(snapshot: &DocumentSnapshot, bound: &Bound, order_by: &[OrderBy]) -> bool {
    let ordering = compare_snapshot_to_bound(snapshot, bound, order_by);
    if bound.inclusive() {
        ordering == Ordering::Greater
    } else {
        ordering != Ordering::Less
    }
}

fn compare_snapshot_to_bound(
    snapshot: &DocumentSnapshot,
    bound: &Bound,
    order_by: &[OrderBy],
) -> Ordering {
    for (index, order) in order_by.iter().enumerate() {
        if index >= bound.values().len() {
            break;
        }

        let bound_value = &bound.values()[index];
        let snapshot_value =
            get_field_value(snapshot, order.field()).unwrap_or_else(FirestoreValue::null);

        let mut ordering = compare_values(&snapshot_value, bound_value).unwrap_or(Ordering::Equal);
        if order.direction() == OrderDirection::Descending {
            ordering = ordering.reverse();
        }

        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firestore::OrderDirection;
    use crate::firestore::api::{Firestore, Query, SnapshotMetadata};
    use crate::firestore::model::{DatabaseId, DocumentKey, FieldPath, ResourcePath};
    use crate::firestore::value::{FirestoreValue, MapValue};
    use crate::test_support::firebase::test_firebase_app_with_api_key;
    use std::collections::BTreeMap;

    fn build_query() -> Query {
        let app = test_firebase_app_with_api_key("query-evaluator");
        let firestore = Firestore::new(app, DatabaseId::new("test", "(default)"));
        let path = ResourcePath::from_string("cities").unwrap();
        Query::new(firestore, path).unwrap()
    }

    fn snapshot_for(id: &str, population: i64) -> DocumentSnapshot {
        let key = DocumentKey::from_string(&format!("cities/{id}")).unwrap();
        let mut map = BTreeMap::new();
        map.insert(
            "population".into(),
            FirestoreValue::from_integer(population),
        );
        let metadata = SnapshotMetadata::new(false, false);
        DocumentSnapshot::new(key, Some(MapValue::new(map)), metadata)
    }

    #[test]
    fn applies_limit_and_ordering() {
        let query = build_query()
            .order_by(
                FieldPath::from_dot_separated("population").unwrap(),
                OrderDirection::Ascending,
            )
            .unwrap()
            .limit(2)
            .unwrap();
        let definition = query.definition();

        let docs = vec![
            snapshot_for("sf", 100),
            snapshot_for("nyc", 50),
            snapshot_for("la", 75),
        ];

        let result = apply_query_to_documents(docs, &definition);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id(), "nyc");
        assert_eq!(result[1].id(), "la");
    }
}
