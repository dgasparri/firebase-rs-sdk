use std::sync::Arc;

use crate::firestore::error::{invalid_argument, FirestoreResult};
use crate::firestore::model::{DocumentKey, FieldPath, ResourcePath};
use crate::firestore::value::FirestoreValue;

use super::snapshot::DocumentSnapshot;
use super::{Firestore, FirestoreDataConverter, TypedDocumentSnapshot};

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
        self.collection_path
            .last_segment()
            .expect("Collection path always ends with an identifier")
    }

    pub fn where_field(
        &self,
        field: impl Into<FieldPath>,
        operator: FilterOperator,
        value: FirestoreValue,
    ) -> FirestoreResult<Self> {
        match operator {
            FilterOperator::ArrayContains
            | FilterOperator::ArrayContainsAny
            | FilterOperator::In
            | FilterOperator::NotIn => {
                return Err(invalid_argument(
                    "operator not yet supported for Firestore queries",
                ))
            }
            _ => {}
        }
        let mut next = self.clone();
        next.filters
            .push(FieldFilter::new(field.into(), operator, value));
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
        let parent_path = self.collection_path.without_last();
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
            collection_path: self.collection_path.clone(),
            parent_path,
            collection_id: self.collection_id().to_string(),
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

#[derive(Clone, Debug)]
pub struct QueryDefinition {
    pub(crate) collection_path: ResourcePath,
    pub(crate) parent_path: ResourcePath,
    pub(crate) collection_id: String,
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
        key.collection_path() == self.collection_path
    }

    pub(crate) fn parent_path(&self) -> &ResourcePath {
        &self.parent_path
    }

    pub(crate) fn collection_id(&self) -> &str {
        &self.collection_id
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

#[derive(Clone, Debug)]
pub struct QuerySnapshot {
    query: Query,
    documents: Vec<DocumentSnapshot>,
}

impl QuerySnapshot {
    pub fn new(query: Query, documents: Vec<DocumentSnapshot>) -> Self {
        Self { query, documents }
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
