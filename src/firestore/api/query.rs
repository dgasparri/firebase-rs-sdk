use std::collections::BTreeMap;
use std::sync::Arc;

use crate::firestore::error::{invalid_argument, FirestoreResult};
use crate::firestore::model::DocumentKey;
use crate::firestore::model::FieldPath;
use crate::firestore::model::ResourcePath;
use crate::firestore::model::Timestamp;
use crate::firestore::value::{FirestoreValue, ValueKind};

use super::snapshot::DocumentSnapshot;
use super::converter::FirestoreDataConverter;
use super::database::Firestore;
use super::snapshot::TypedDocumentSnapshot;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterOperator {
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Equal,
    NotEqual,
    ArrayContains,
    ArrayContainsAny,
    In,
    NotIn,
}

impl FilterOperator {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            FilterOperator::LessThan => "LESS_THAN",
            FilterOperator::LessThanOrEqual => "LESS_THAN_OR_EQUAL",
            FilterOperator::GreaterThan => "GREATER_THAN",
            FilterOperator::GreaterThanOrEqual => "GREATER_THAN_OR_EQUAL",
            FilterOperator::Equal => "EQUAL",
            FilterOperator::NotEqual => "NOT_EQUAL",
            FilterOperator::ArrayContains => "ARRAY_CONTAINS",
            FilterOperator::ArrayContainsAny => "ARRAY_CONTAINS_ANY",
            FilterOperator::In => "IN",
            FilterOperator::NotIn => "NOT_IN",
        }
    }

    pub(crate) fn keyword(&self) -> &'static str {
        match self {
            FilterOperator::LessThan => "<",
            FilterOperator::LessThanOrEqual => "<=",
            FilterOperator::GreaterThan => ">",
            FilterOperator::GreaterThanOrEqual => ">=",
            FilterOperator::Equal => "==",
            FilterOperator::NotEqual => "!=",
            FilterOperator::ArrayContains => "array-contains",
            FilterOperator::ArrayContainsAny => "array-contains-any",
            FilterOperator::In => "in",
            FilterOperator::NotIn => "not-in",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderDirection {
    Ascending,
    Descending,
}

impl OrderDirection {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            OrderDirection::Ascending => "ASCENDING",
            OrderDirection::Descending => "DESCENDING",
        }
    }

    fn flipped(&self) -> Self {
        match self {
            OrderDirection::Ascending => OrderDirection::Descending,
            OrderDirection::Descending => OrderDirection::Ascending,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitType {
    First,
    Last,
}

#[derive(Clone, Debug)]
pub(crate) struct FieldFilter {
    field: FieldPath,
    operator: FilterOperator,
    value: FirestoreValue,
}

impl FieldFilter {
    fn new(field: FieldPath, operator: FilterOperator, value: FirestoreValue) -> Self {
        Self {
            field,
            operator,
            value,
        }
    }

    pub(crate) fn field(&self) -> &FieldPath {
        &self.field
    }

    pub(crate) fn operator(&self) -> FilterOperator {
        self.operator
    }

    pub(crate) fn value(&self) -> &FirestoreValue {
        &self.value
    }
}

#[derive(Clone, Debug)]
pub(crate) struct OrderBy {
    field: FieldPath,
    direction: OrderDirection,
}

impl OrderBy {
    fn new(field: FieldPath, direction: OrderDirection) -> Self {
        Self { field, direction }
    }

    pub(crate) fn field(&self) -> &FieldPath {
        &self.field
    }

    pub(crate) fn direction(&self) -> OrderDirection {
        self.direction
    }

    fn flipped(&self) -> Self {
        Self {
            field: self.field.clone(),
            direction: self.direction.flipped(),
        }
    }

    fn is_document_id(&self) -> bool {
        self.field == FieldPath::document_id()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Bound {
    values: Vec<FirestoreValue>,
    inclusive: bool,
}

impl Bound {
    fn new(values: Vec<FirestoreValue>, inclusive: bool) -> Self {
        Self { values, inclusive }
    }

    pub(crate) fn values(&self) -> &[FirestoreValue] {
        &self.values
    }

    pub(crate) fn inclusive(&self) -> bool {
        self.inclusive
    }
}

#[derive(Clone, Debug)]
pub struct Query {
    firestore: Firestore,
    collection_path: ResourcePath,
    collection_group: Option<String>,
    filters: Vec<FieldFilter>,
    explicit_order_by: Vec<OrderBy>,
    limit: Option<u32>,
    limit_type: LimitType,
    start_at: Option<Bound>,
    end_at: Option<Bound>,
    projection: Option<Vec<FieldPath>>,
}

impl Query {
    pub(crate) fn new(
        firestore: Firestore,
        collection_path: ResourcePath,
    ) -> FirestoreResult<Self> {
        if collection_path.len() % 2 == 0 {
            return Err(invalid_argument(
                "Queries must reference a collection (odd number of path segments)",
            ));
        }
        Ok(Self {
            firestore,
            collection_path,
            collection_group: None,
            filters: Vec::new(),
            explicit_order_by: Vec::new(),
            limit: None,
            limit_type: LimitType::First,
            start_at: None,
            end_at: None,
            projection: None,
        })
    }

    pub(crate) fn new_collection_group(
        firestore: Firestore,
        collection_id: String,
    ) -> FirestoreResult<Self> {
        if collection_id.is_empty() {
            return Err(invalid_argument("Collection ID must not be empty"));
        }
        if collection_id.contains('/') {
            return Err(invalid_argument(
                "Collection ID must not contain '/' characters",
            ));
        }
        Ok(Self {
            firestore,
            collection_path: ResourcePath::root(),
            collection_group: Some(collection_id),
            filters: Vec::new(),
            explicit_order_by: Vec::new(),
            limit: None,
            limit_type: LimitType::First,
            start_at: None,
            end_at: None,
            projection: None,
        })
    }

    pub fn firestore(&self) -> &Firestore {
        &self.firestore
    }

    pub fn collection_path(&self) -> &ResourcePath {
        &self.collection_path
    }

    pub fn collection_id(&self) -> &str {
        match &self.collection_group {
            Some(group) => group.as_str(),
            None => self
                .collection_path
                .last_segment()
                .expect("Collection path always ends with an identifier"),
        }
    }

    pub fn where_field(
        &self,
        field: impl Into<FieldPath>,
        operator: FilterOperator,
        value: FirestoreValue,
    ) -> FirestoreResult<Self> {
        let field_path = field.into();
        self.validate_filter(&field_path, operator, &value)?;
        let mut next = self.clone();
        next.filters
            .push(FieldFilter::new(field_path, operator, value));
        Ok(next)
    }

    pub fn order_by(
        &self,
        field: impl Into<FieldPath>,
        direction: OrderDirection,
    ) -> FirestoreResult<Self> {
        if self.start_at.is_some() || self.end_at.is_some() {
            return Err(invalid_argument(
                "order_by clauses must be added before specifying start/end cursors",
            ));
        }
        let mut next = self.clone();
        next.explicit_order_by
            .push(OrderBy::new(field.into(), direction));
        Ok(next)
    }

    pub fn limit(&self, value: u32) -> FirestoreResult<Self> {
        if value == 0 {
            return Err(invalid_argument("limit must be greater than zero"));
        }
        let mut next = self.clone();
        next.limit = Some(value);
        next.limit_type = LimitType::First;
        Ok(next)
    }

    pub fn limit_to_last(&self, value: u32) -> FirestoreResult<Self> {
        if value == 0 {
            return Err(invalid_argument("limit must be greater than zero"));
        }
        if self.explicit_order_by.is_empty() {
            return Err(invalid_argument(
                "limit_to_last requires at least one order_by clause",
            ));
        }
        let mut next = self.clone();
        next.limit = Some(value);
        next.limit_type = LimitType::Last;
        Ok(next)
    }

    pub fn start_at(&self, values: Vec<FirestoreValue>) -> FirestoreResult<Self> {
        self.apply_start_bound(values, true)
    }

    pub fn start_after(&self, values: Vec<FirestoreValue>) -> FirestoreResult<Self> {
        self.apply_start_bound(values, false)
    }

    pub fn end_at(&self, values: Vec<FirestoreValue>) -> FirestoreResult<Self> {
        self.apply_end_bound(values, true)
    }

    pub fn end_before(&self, values: Vec<FirestoreValue>) -> FirestoreResult<Self> {
        self.apply_end_bound(values, false)
    }

    pub fn select<I>(&self, fields: I) -> FirestoreResult<Self>
    where
        I: IntoIterator<Item = FieldPath>,
    {
        if self.projection.is_some() {
            return Err(invalid_argument(
                "projection already specified for this query",
            ));
        }
        let mut unique = Vec::new();
        for field in fields {
            if !unique.iter().any(|existing: &FieldPath| existing == &field) {
                unique.push(field);
            }
        }
        if unique.is_empty() {
            return Err(invalid_argument(
                "projection must specify at least one field",
            ));
        }
        let mut next = self.clone();
        next.projection = Some(unique);
        Ok(next)
    }

    pub(crate) fn definition(&self) -> QueryDefinition {
        let (collection_path, parent_path, collection_group) = match &self.collection_group {
            Some(group) => (
                self.collection_path.clone(),
                self.collection_path.clone(),
                Some(group.clone()),
            ),
            None => {
                let parent_path = self.collection_path.without_last();
                (self.collection_path.clone(), parent_path, None)
            }
        };
        let normalized_order = self.normalized_order_by();

        let mut request_order = normalized_order.clone();
        let mut request_start = self.start_at.clone();
        let mut request_end = self.end_at.clone();

        if self.limit_type == LimitType::Last {
            request_order = request_order
                .into_iter()
                .map(|order| order.flipped())
                .collect();
            request_start = self.end_at.clone();
            request_end = self.start_at.clone();
        }

        QueryDefinition {
            collection_path,
            parent_path,
            collection_id: self.collection_id().to_string(),
            collection_group,
            filters: self.filters.clone(),
            request_order_by: request_order,
            result_order_by: normalized_order,
            limit: self.limit,
            limit_type: self.limit_type,
            request_start_at: request_start,
            request_end_at: request_end,
            result_start_at: self.start_at.clone(),
            result_end_at: self.end_at.clone(),
            projection: self.projection.clone(),
        }
    }

    pub fn with_converter<C>(&self, converter: C) -> ConvertedQuery<C>
    where
        C: FirestoreDataConverter,
    {
        ConvertedQuery::new(self.clone(), Arc::new(converter))
    }

    fn apply_start_bound(
        &self,
        values: Vec<FirestoreValue>,
        inclusive: bool,
    ) -> FirestoreResult<Self> {
        if values.is_empty() {
            return Err(invalid_argument(
                "startAt/startAfter require at least one cursor value",
            ));
        }
        let mut next = self.clone();
        next.start_at = Some(Bound::new(values, inclusive));
        Ok(next)
    }

    fn apply_end_bound(
        &self,
        values: Vec<FirestoreValue>,
        inclusive: bool,
    ) -> FirestoreResult<Self> {
        if values.is_empty() {
            return Err(invalid_argument(
                "endAt/endBefore require at least one cursor value",
            ));
        }
        let mut next = self.clone();
        next.end_at = Some(Bound::new(values, inclusive));
        Ok(next)
    }

    fn normalized_order_by(&self) -> Vec<OrderBy> {
        let mut order = self.explicit_order_by.clone();
        if !order.iter().any(|existing| existing.is_document_id()) {
            order.push(OrderBy::new(
                FieldPath::document_id(),
                OrderDirection::Ascending,
            ));
        }
        order
    }
}

const MAX_DISJUNCTIVE_VALUES: usize = 10;

impl Query {
    fn validate_filter(
        &self,
        field: &FieldPath,
        operator: FilterOperator,
        value: &FirestoreValue,
    ) -> FirestoreResult<()> {
        if field == &FieldPath::document_id() {
            match operator {
                FilterOperator::ArrayContains | FilterOperator::ArrayContainsAny => {
                    return Err(invalid_argument(
                        "Invalid query. You can't use array membership operators with documentId().",
                    ));
                }
                _ => {}
            }
        }

        match operator {
            FilterOperator::ArrayContains => {
                ensure_value_supported(operator, value)?;
            }
            FilterOperator::ArrayContainsAny | FilterOperator::In | FilterOperator::NotIn => {
                ensure_disjunctive_filter(operator, value)?;
            }
            FilterOperator::NotEqual => {
                if is_nan(value) {
                    return Err(invalid_argument(
                        "Invalid query. You cannot use '!=' filters with NaN values.",
                    ));
                }
            }
            _ => {}
        }

        // Prevent mixing inequality operators on different fields.
        if is_inequality(operator) {
            if let Some(existing) = self.inequality_field() {
                if existing != field.canonical_string() {
                    return Err(invalid_argument(
                        "Invalid query. All inequality filters must be on the same field.",
                    ));
                }
            }
        }

        Ok(())
    }

    fn inequality_field(&self) -> Option<String> {
        self.filters.iter().find_map(|filter| {
            if is_inequality(filter.operator()) {
                Some(filter.field().canonical_string())
            } else {
                None
            }
        })
    }
}

fn is_inequality(operator: FilterOperator) -> bool {
    matches!(
        operator,
        FilterOperator::LessThan
            | FilterOperator::LessThanOrEqual
            | FilterOperator::GreaterThan
            | FilterOperator::GreaterThanOrEqual
            | FilterOperator::NotEqual
            | FilterOperator::NotIn
    )
}

fn ensure_value_supported(operator: FilterOperator, value: &FirestoreValue) -> FirestoreResult<()> {
    if is_nan(value) || matches!(value.kind(), ValueKind::Null) {
        return Err(invalid_argument(format!(
            "Invalid query. You cannot use '{}' filters with null or NaN values.",
            operator.keyword()
        )));
    }
    Ok(())
}

fn ensure_disjunctive_filter(
    operator: FilterOperator,
    value: &FirestoreValue,
) -> FirestoreResult<()> {
    let array = match value.kind() {
        ValueKind::Array(array) => array,
        _ => {
            return Err(invalid_argument(format!(
                "Invalid query. A non-empty array is required for '{}' filters.",
                operator.keyword()
            )))
        }
    };

    let elements = array.values();
    if elements.is_empty() {
        return Err(invalid_argument(format!(
            "Invalid query. A non-empty array is required for '{}' filters.",
            operator.keyword()
        )));
    }
    if elements.len() > MAX_DISJUNCTIVE_VALUES {
        return Err(invalid_argument(format!(
            "Invalid query. '{}' filters support a maximum of {} elements.",
            operator.keyword(),
            MAX_DISJUNCTIVE_VALUES
        )));
    }

    for element in elements {
        if is_nan(element) || matches!(element.kind(), ValueKind::Null) {
            return Err(invalid_argument(format!(
                "Invalid query. '{}' filters cannot contain 'null' or 'NaN' values.",
                operator.keyword()
            )));
        }
    }

    Ok(())
}

fn is_nan(value: &FirestoreValue) -> bool {
    matches!(value.kind(), ValueKind::Double(number) if number.is_nan())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::FirebaseApp;
    use crate::app::FirebaseAppConfig;
    use crate::app::FirebaseOptions;
    use crate::component::ComponentContainer;
    use crate::firestore::api::snapshot::SnapshotMetadata;
    use crate::firestore::Firestore;
    use crate::firestore::{DatabaseId, DocumentKey, FieldPath, ResourcePath};
    use crate::firestore::MapValue;
    use std::collections::BTreeMap;

    fn build_firestore() -> Firestore {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let config = FirebaseAppConfig::new("query-test", false);
        let container = ComponentContainer::new("query-test");
        let app = FirebaseApp::new(options, config, container);
        Firestore::new(app, DatabaseId::new("project", "(default)"))
    }

    fn build_query() -> Query {
        let firestore = build_firestore();
        Query::new(firestore, ResourcePath::from_string("cities").unwrap()).unwrap()
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
    fn array_contains_rejects_null() {
        let query = build_query();
        let err = query
            .where_field(
                FieldPath::from_dot_separated("tags").unwrap(),
                FilterOperator::ArrayContains,
                FirestoreValue::null(),
            )
            .unwrap_err();
        assert_eq!(err.code_str(), "firestore/invalid-argument");
    }

    #[test]
    fn array_contains_any_requires_array() {
        let query = build_query();
        let err = query
            .where_field(
                FieldPath::from_dot_separated("tags").unwrap(),
                FilterOperator::ArrayContainsAny,
                FirestoreValue::from_string("coastal"),
            )
            .unwrap_err();
        assert_eq!(err.code_str(), "firestore/invalid-argument");
    }

    #[test]
    fn in_filter_rejects_large_arrays() {
        let query = build_query();
        let mut values = Vec::new();
        for i in 0..11 {
            values.push(FirestoreValue::from_integer(i));
        }
        let err = query
            .where_field(
                FieldPath::from_dot_separated("rank").unwrap(),
                FilterOperator::In,
                FirestoreValue::from_array(values),
            )
            .unwrap_err();
        assert_eq!(err.code_str(), "firestore/invalid-argument");
    }

    #[test]
    fn in_filter_accepts_valid_values() {
        let query = build_query();
        let values = FirestoreValue::from_array(vec![
            FirestoreValue::from_integer(1),
            FirestoreValue::from_integer(2),
        ]);
        let _ = query
            .where_field(
                FieldPath::from_dot_separated("rank").unwrap(),
                FilterOperator::In,
                values,
            )
            .expect("valid in filter");
    }

    #[test]
    fn document_id_disallows_array_contains() {
        let query = build_query();
        let err = query
            .where_field(
                FieldPath::document_id(),
                FilterOperator::ArrayContains,
                FirestoreValue::from_string("foo"),
            )
            .unwrap_err();
        assert_eq!(err.code_str(), "firestore/invalid-argument");
    }

    #[test]
    fn collection_group_query_definition_marks_descendants() {
        let firestore = build_firestore();
        let query = Query::new_collection_group(firestore, "landmarks".to_string()).unwrap();
        let definition = query.definition();
        assert_eq!(definition.collection_group(), Some("landmarks"));
        assert_eq!(definition.collection_id(), "landmarks");
        assert!(definition.parent_path().is_empty());

        let matching = DocumentKey::from_string("cities/sf/landmarks/golden_gate").unwrap();
        let non_matching = DocumentKey::from_string("cities/sf/attractions/golden_gate").unwrap();
        assert!(definition.matches_collection(&matching));
        assert!(!definition.matches_collection(&non_matching));
    }

    #[test]
    fn compute_doc_changes_tracks_add_modify_remove() {
        let previous = vec![snapshot_for("sf", 100), snapshot_for("la", 200)];
        let current = vec![
            snapshot_for("la", 200),
            snapshot_for("sf", 150),
            snapshot_for("ny", 50),
        ];

        let changes = compute_doc_changes(Some(&previous), &current);
        assert_eq!(changes.len(), 3);

        assert_eq!(changes[0].change_type(), DocumentChangeType::Modified);
        assert_eq!(changes[0].old_index(), 1);
        assert_eq!(changes[0].new_index(), 0);

        assert_eq!(changes[1].change_type(), DocumentChangeType::Modified);
        assert_eq!(changes[1].old_index(), 0);
        assert_eq!(changes[1].new_index(), 1);

        assert_eq!(changes[2].change_type(), DocumentChangeType::Added);
        assert_eq!(changes[2].old_index(), -1);
        assert_eq!(changes[2].new_index(), 2);
    }

    #[test]
    fn compute_doc_changes_handles_removals() {
        let previous = vec![snapshot_for("sf", 100)];
        let current = Vec::new();

        let changes = compute_doc_changes(Some(&previous), &current);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_type(), DocumentChangeType::Removed);
        assert_eq!(changes[0].old_index(), 0);
        assert_eq!(changes[0].new_index(), -1);
        assert_eq!(changes[0].doc().id(), "sf");
    }

    #[test]
    fn compute_doc_changes_noop_is_empty() {
        let previous = vec![snapshot_for("sf", 100), snapshot_for("la", 200)];
        let current = previous.clone();

        let changes = compute_doc_changes(Some(&previous), &current);
        assert!(changes.is_empty());
    }

    #[test]
    fn compute_doc_changes_handles_limit_to_last_reorder() {
        let previous = vec![snapshot_for("a", 100), snapshot_for("b", 200)];
        let current = vec![snapshot_for("b", 200), snapshot_for("c", 300)];

        let changes = compute_doc_changes(Some(&previous), &current);
        assert_eq!(changes.len(), 3);

        assert_eq!(changes[0].change_type(), DocumentChangeType::Modified);
        assert_eq!(changes[0].old_index(), 1);
        assert_eq!(changes[0].new_index(), 0);
        assert_eq!(changes[0].doc().id(), "b");

        assert_eq!(changes[1].change_type(), DocumentChangeType::Added);
        assert_eq!(changes[1].old_index(), -1);
        assert_eq!(changes[1].new_index(), 1);
        assert_eq!(changes[1].doc().id(), "c");

        assert_eq!(changes[2].change_type(), DocumentChangeType::Removed);
        assert_eq!(changes[2].old_index(), 0);
        assert_eq!(changes[2].new_index(), -1);
        assert_eq!(changes[2].doc().id(), "a");
    }

    #[test]
    fn compute_doc_changes_handles_multi_ordering() {
        let doc = |id: &str, team: i64, score: i64| {
            let key = DocumentKey::from_string(&format!("games/{id}")).unwrap();
            let mut fields = BTreeMap::new();
            fields.insert("team".into(), FirestoreValue::from_integer(team));
            fields.insert("score".into(), FirestoreValue::from_integer(score));
            DocumentSnapshot::new(
                key,
                Some(MapValue::new(fields)),
                SnapshotMetadata::new(false, false),
            )
        };

        let previous = vec![doc("a", 1, 10), doc("b", 1, 20), doc("c", 2, 5)];
        let current = vec![doc("b", 1, 20), doc("c", 2, 5), doc("d", 2, 15)];

        let changes = compute_doc_changes(Some(&previous), &current);
        assert_eq!(changes.len(), 4);

        assert_eq!(changes[0].change_type(), DocumentChangeType::Modified);
        assert_eq!(changes[0].doc().id(), "b");
        assert_eq!(changes[0].old_index(), 1);
        assert_eq!(changes[0].new_index(), 0);

        assert_eq!(changes[1].change_type(), DocumentChangeType::Modified);
        assert_eq!(changes[1].doc().id(), "c");
        assert_eq!(changes[1].old_index(), 2);
        assert_eq!(changes[1].new_index(), 1);

        assert_eq!(changes[2].change_type(), DocumentChangeType::Added);
        assert_eq!(changes[2].doc().id(), "d");
        assert_eq!(changes[2].old_index(), -1);
        assert_eq!(changes[2].new_index(), 2);

        assert_eq!(changes[3].change_type(), DocumentChangeType::Removed);
        assert_eq!(changes[3].doc().id(), "a");
        assert_eq!(changes[3].old_index(), 0);
        assert_eq!(changes[3].new_index(), -1);
    }
}

#[derive(Clone, Debug)]
pub struct QueryDefinition {
    pub(crate) collection_path: ResourcePath,
    pub(crate) parent_path: ResourcePath,
    pub(crate) collection_id: String,
    pub(crate) collection_group: Option<String>,
    pub(crate) filters: Vec<FieldFilter>,
    pub(crate) request_order_by: Vec<OrderBy>,
    pub(crate) result_order_by: Vec<OrderBy>,
    pub(crate) limit: Option<u32>,
    pub(crate) limit_type: LimitType,
    pub(crate) request_start_at: Option<Bound>,
    pub(crate) request_end_at: Option<Bound>,
    pub(crate) result_start_at: Option<Bound>,
    pub(crate) result_end_at: Option<Bound>,
    pub(crate) projection: Option<Vec<FieldPath>>,
}

impl QueryDefinition {
    pub(crate) fn matches_collection(&self, key: &DocumentKey) -> bool {
        match &self.collection_group {
            Some(group) => key
                .collection_path()
                .last_segment()
                .map(|segment| segment == group)
                .unwrap_or(false),
            None => key.collection_path() == self.collection_path,
        }
    }

    pub(crate) fn parent_path(&self) -> &ResourcePath {
        &self.parent_path
    }

    pub(crate) fn collection_id(&self) -> &str {
        &self.collection_id
    }

    pub(crate) fn collection_group(&self) -> Option<&str> {
        self.collection_group.as_deref()
    }

    pub(crate) fn filters(&self) -> &[FieldFilter] {
        &self.filters
    }

    pub(crate) fn request_order_by(&self) -> &[OrderBy] {
        &self.request_order_by
    }

    pub(crate) fn result_order_by(&self) -> &[OrderBy] {
        &self.result_order_by
    }

    pub(crate) fn limit(&self) -> Option<u32> {
        self.limit
    }

    pub(crate) fn limit_type(&self) -> LimitType {
        self.limit_type
    }

    pub(crate) fn request_start_at(&self) -> Option<&Bound> {
        self.request_start_at.as_ref()
    }

    pub(crate) fn request_end_at(&self) -> Option<&Bound> {
        self.request_end_at.as_ref()
    }

    pub(crate) fn result_start_at(&self) -> Option<&Bound> {
        self.result_start_at.as_ref()
    }

    pub(crate) fn result_end_at(&self) -> Option<&Bound> {
        self.result_end_at.as_ref()
    }

    pub(crate) fn projection(&self) -> Option<&[FieldPath]> {
        self.projection.as_deref()
    }
}

#[derive(Clone)]
pub struct ConvertedQuery<C>
where
    C: FirestoreDataConverter,
{
    inner: Query,
    converter: Arc<C>,
}

impl<C> ConvertedQuery<C>
where
    C: FirestoreDataConverter,
{
    pub(crate) fn new(inner: Query, converter: Arc<C>) -> Self {
        Self { inner, converter }
    }

    pub fn raw(&self) -> &Query {
        &self.inner
    }

    pub fn where_field(
        &self,
        field: impl Into<FieldPath>,
        operator: FilterOperator,
        value: FirestoreValue,
    ) -> FirestoreResult<Self> {
        let query = self.inner.where_field(field, operator, value)?;
        Ok(Self::new(query, Arc::clone(&self.converter)))
    }

    pub fn order_by(
        &self,
        field: impl Into<FieldPath>,
        direction: OrderDirection,
    ) -> FirestoreResult<Self> {
        let query = self.inner.order_by(field, direction)?;
        Ok(Self::new(query, Arc::clone(&self.converter)))
    }

    pub fn limit(&self, value: u32) -> FirestoreResult<Self> {
        let query = self.inner.limit(value)?;
        Ok(Self::new(query, Arc::clone(&self.converter)))
    }

    pub fn limit_to_last(&self, value: u32) -> FirestoreResult<Self> {
        let query = self.inner.limit_to_last(value)?;
        Ok(Self::new(query, Arc::clone(&self.converter)))
    }

    pub fn start_at(&self, values: Vec<FirestoreValue>) -> FirestoreResult<Self> {
        let query = self.inner.start_at(values)?;
        Ok(Self::new(query, Arc::clone(&self.converter)))
    }

    pub fn start_after(&self, values: Vec<FirestoreValue>) -> FirestoreResult<Self> {
        let query = self.inner.start_after(values)?;
        Ok(Self::new(query, Arc::clone(&self.converter)))
    }

    pub fn end_at(&self, values: Vec<FirestoreValue>) -> FirestoreResult<Self> {
        let query = self.inner.end_at(values)?;
        Ok(Self::new(query, Arc::clone(&self.converter)))
    }

    pub fn end_before(&self, values: Vec<FirestoreValue>) -> FirestoreResult<Self> {
        let query = self.inner.end_before(values)?;
        Ok(Self::new(query, Arc::clone(&self.converter)))
    }

    pub fn select<I>(&self, fields: I) -> FirestoreResult<Self>
    where
        I: IntoIterator<Item = FieldPath>,
    {
        let query = self.inner.select(fields)?;
        Ok(Self::new(query, Arc::clone(&self.converter)))
    }

    pub(crate) fn converter(&self) -> Arc<C> {
        Arc::clone(&self.converter)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuerySnapshotMetadata {
    from_cache: bool,
    has_pending_writes: bool,
    sync_state_changed: bool,
    resume_token: Option<Vec<u8>>,
    snapshot_version: Option<Timestamp>,
}

impl QuerySnapshotMetadata {
    pub fn new(
        from_cache: bool,
        has_pending_writes: bool,
        sync_state_changed: bool,
        resume_token: Option<Vec<u8>>,
        snapshot_version: Option<Timestamp>,
    ) -> Self {
        Self {
            from_cache,
            has_pending_writes,
            sync_state_changed,
            resume_token,
            snapshot_version,
        }
    }

    pub fn from_cache(&self) -> bool {
        self.from_cache
    }

    pub fn has_pending_writes(&self) -> bool {
        self.has_pending_writes
    }

    pub fn sync_state_changed(&self) -> bool {
        self.sync_state_changed
    }

    pub fn resume_token(&self) -> Option<&[u8]> {
        self.resume_token.as_deref()
    }

    pub fn snapshot_version(&self) -> Option<&Timestamp> {
        self.snapshot_version.as_ref()
    }

    pub(crate) fn set_sync_state_changed(&mut self, value: bool) {
        self.sync_state_changed = value;
    }
}

#[derive(Clone)]
pub struct QuerySnapshot {
    query: Query,
    documents: Vec<DocumentSnapshot>,
    metadata: QuerySnapshotMetadata,
    doc_changes: Vec<QueryDocumentChange>,
}

impl QuerySnapshot {
    pub fn new(
        query: Query,
        documents: Vec<DocumentSnapshot>,
        metadata: QuerySnapshotMetadata,
        doc_changes: Vec<QueryDocumentChange>,
    ) -> Self {
        Self {
            query,
            documents,
            metadata,
            doc_changes,
        }
    }

    pub fn query(&self) -> &Query {
        &self.query
    }

    pub fn documents(&self) -> &[DocumentSnapshot] {
        &self.documents
    }

    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    pub fn len(&self) -> usize {
        self.documents.len()
    }

    pub fn metadata(&self) -> &QuerySnapshotMetadata {
        &self.metadata
    }

    pub fn from_cache(&self) -> bool {
        self.metadata.from_cache()
    }

    pub fn has_pending_writes(&self) -> bool {
        self.metadata.has_pending_writes()
    }

    pub fn resume_token(&self) -> Option<&[u8]> {
        self.metadata.resume_token()
    }

    pub fn snapshot_version(&self) -> Option<&Timestamp> {
        self.metadata.snapshot_version()
    }

    pub fn doc_changes(&self) -> &[QueryDocumentChange] {
        &self.doc_changes
    }

    pub fn into_documents(self) -> Vec<DocumentSnapshot> {
        self.documents
    }
}

impl IntoIterator for QuerySnapshot {
    type Item = DocumentSnapshot;
    type IntoIter = std::vec::IntoIter<DocumentSnapshot>;

    fn into_iter(self) -> Self::IntoIter {
        self.documents.into_iter()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentChangeType {
    Added,
    Modified,
    Removed,
}

#[derive(Clone, Debug)]
pub struct QueryDocumentChange {
    change_type: DocumentChangeType,
    doc: DocumentSnapshot,
    old_index: i32,
    new_index: i32,
}

impl QueryDocumentChange {
    fn added(doc: DocumentSnapshot, new_index: usize) -> Self {
        Self {
            change_type: DocumentChangeType::Added,
            doc,
            old_index: -1,
            new_index: new_index as i32,
        }
    }

    fn modified(doc: DocumentSnapshot, old_index: usize, new_index: usize) -> Self {
        Self {
            change_type: DocumentChangeType::Modified,
            doc,
            old_index: old_index as i32,
            new_index: new_index as i32,
        }
    }

    fn removed(doc: DocumentSnapshot, old_index: usize) -> Self {
        Self {
            change_type: DocumentChangeType::Removed,
            doc,
            old_index: old_index as i32,
            new_index: -1,
        }
    }

    pub fn change_type(&self) -> DocumentChangeType {
        self.change_type
    }

    pub fn doc(&self) -> &DocumentSnapshot {
        &self.doc
    }

    pub fn old_index(&self) -> i32 {
        self.old_index
    }

    pub fn new_index(&self) -> i32 {
        self.new_index
    }
}

pub(crate) fn compute_doc_changes(
    previous: Option<&[DocumentSnapshot]>,
    current: &[DocumentSnapshot],
) -> Vec<QueryDocumentChange> {
    match previous {
        None => current
            .iter()
            .enumerate()
            .map(|(index, doc)| QueryDocumentChange::added(doc.clone(), index))
            .collect(),
        Some(prev_docs) => {
            if prev_docs.is_empty() {
                return current
                    .iter()
                    .enumerate()
                    .map(|(index, doc)| QueryDocumentChange::added(doc.clone(), index))
                    .collect();
            }

            let mut previous_map: BTreeMap<DocumentKey, usize> = BTreeMap::new();
            for (index, doc) in prev_docs.iter().enumerate() {
                previous_map.insert(doc.document_key().clone(), index);
            }

            let mut changes = Vec::new();

            for (new_index, doc) in current.iter().enumerate() {
                let key = doc.document_key().clone();
                if let Some(old_index) = previous_map.remove(&key) {
                    let prev_doc = &prev_docs[old_index];
                    if !document_snapshots_equal(prev_doc, doc) || old_index != new_index {
                        changes.push(QueryDocumentChange::modified(
                            doc.clone(),
                            old_index,
                            new_index,
                        ));
                    }
                } else {
                    changes.push(QueryDocumentChange::added(doc.clone(), new_index));
                }
            }

            for (_, old_index) in previous_map.into_iter() {
                let prev_doc = prev_docs[old_index].clone();
                changes.push(QueryDocumentChange::removed(prev_doc, old_index));
            }

            changes
        }
    }
}

fn document_snapshots_equal(left: &DocumentSnapshot, right: &DocumentSnapshot) -> bool {
    left.map_value() == right.map_value() && left.metadata() == right.metadata()
}

#[derive(Clone)]
pub struct TypedQuerySnapshot<C>
where
    C: FirestoreDataConverter,
{
    base: QuerySnapshot,
    converter: Arc<C>,
}

impl<C> TypedQuerySnapshot<C>
where
    C: FirestoreDataConverter,
{
    pub(crate) fn new(base: QuerySnapshot, converter: Arc<C>) -> Self {
        Self { base, converter }
    }

    pub fn raw(&self) -> &QuerySnapshot {
        &self.base
    }

    pub fn documents(&self) -> Vec<TypedDocumentSnapshot<C>> {
        let converter = Arc::clone(&self.converter);
        self.base
            .documents
            .iter()
            .cloned()
            .map(|snapshot| snapshot.into_typed(Arc::clone(&converter)))
            .collect()
    }

    pub fn doc_changes(&self) -> Vec<TypedQueryDocumentChange<C>> {
        let converter = Arc::clone(&self.converter);
        self.base
            .doc_changes()
            .iter()
            .cloned()
            .map(|change| {
                let typed_doc = change.doc.clone().into_typed(Arc::clone(&converter));
                TypedQueryDocumentChange::new(
                    change.change_type,
                    typed_doc,
                    change.old_index,
                    change.new_index,
                )
            })
            .collect()
    }

    pub fn metadata(&self) -> &QuerySnapshotMetadata {
        self.base.metadata()
    }
}

impl<C> IntoIterator for TypedQuerySnapshot<C>
where
    C: FirestoreDataConverter,
{
    type Item = TypedDocumentSnapshot<C>;
    type IntoIter = std::vec::IntoIter<TypedDocumentSnapshot<C>>;

    fn into_iter(self) -> Self::IntoIter {
        let converter = Arc::clone(&self.converter);
        self.base
            .into_documents()
            .into_iter()
            .map(|snapshot| snapshot.into_typed(Arc::clone(&converter)))
            .collect::<Vec<_>>()
            .into_iter()
    }
}

#[derive(Clone)]
pub struct TypedQueryDocumentChange<C>
where
    C: FirestoreDataConverter,
{
    change_type: DocumentChangeType,
    doc: TypedDocumentSnapshot<C>,
    old_index: i32,
    new_index: i32,
}

impl<C> TypedQueryDocumentChange<C>
where
    C: FirestoreDataConverter,
{
    fn new(
        change_type: DocumentChangeType,
        doc: TypedDocumentSnapshot<C>,
        old_index: i32,
        new_index: i32,
    ) -> Self {
        Self {
            change_type,
            doc,
            old_index,
            new_index,
        }
    }

    pub fn change_type(&self) -> DocumentChangeType {
        self.change_type
    }

    pub fn doc(&self) -> &TypedDocumentSnapshot<C> {
        &self.doc
    }

    pub fn old_index(&self) -> i32 {
        self.old_index
    }

    pub fn new_index(&self) -> i32 {
        self.new_index
    }
}
