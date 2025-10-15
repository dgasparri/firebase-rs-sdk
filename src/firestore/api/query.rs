use std::sync::Arc;

use crate::firestore::error::{invalid_argument, FirestoreResult};
use crate::firestore::model::{DocumentKey, ResourcePath};

use super::snapshot::DocumentSnapshot;
use super::{Firestore, FirestoreDataConverter, TypedDocumentSnapshot};

/// A Firestore query targeting a specific collection.
///
/// Queries currently support collection scans without filters. Additional query
/// operators (where, order_by, limit, etc.) will be introduced as the port
/// progresses.
#[derive(Clone, Debug)]
pub struct Query {
    firestore: Firestore,
    collection_path: ResourcePath,
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
        })
    }

    /// Returns the Firestore instance that created this query.
    pub fn firestore(&self) -> &Firestore {
        &self.firestore
    }

    /// Returns the full resource path to the targeted collection.
    pub fn collection_path(&self) -> &ResourcePath {
        &self.collection_path
    }

    /// The identifier (last segment) of the targeted collection.
    pub fn collection_id(&self) -> &str {
        self.collection_path
            .last_segment()
            .expect("Collection path always ends with an identifier")
    }

    pub(crate) fn definition(&self) -> QueryDefinition {
        QueryDefinition {
            collection_path: self.collection_path.clone(),
        }
    }

    /// Attaches a converter to this query.
    pub fn with_converter<C>(&self, converter: C) -> ConvertedQuery<C>
    where
        C: FirestoreDataConverter,
    {
        ConvertedQuery::new(self.clone(), Arc::new(converter))
    }
}

/// Internal representation of the collection targeted by a query.
#[derive(Clone, Debug)]
pub struct QueryDefinition {
    pub(crate) collection_path: ResourcePath,
}

impl QueryDefinition {
    pub(crate) fn matches(&self, key: &DocumentKey) -> bool {
        key.collection_path() == self.collection_path
    }
}

/// A query with an attached data converter for typed access.
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

    /// Returns the untyped query backing this converted query.
    pub fn raw(&self) -> &Query {
        &self.inner
    }

    pub(crate) fn converter(&self) -> Arc<C> {
        Arc::clone(&self.converter)
    }
}

/// A snapshot containing the results of executing a query.
#[derive(Clone, Debug)]
pub struct QuerySnapshot {
    query: Query,
    documents: Vec<DocumentSnapshot>,
}

impl QuerySnapshot {
    pub fn new(query: Query, documents: Vec<DocumentSnapshot>) -> Self {
        Self { query, documents }
    }

    /// Returns the query used to obtain this snapshot.
    pub fn query(&self) -> &Query {
        &self.query
    }

    /// Returns all document snapshots returned by the query.
    pub fn documents(&self) -> &[DocumentSnapshot] {
        &self.documents
    }

    /// Returns whether the snapshot contains no documents.
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    /// Returns the number of documents.
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Consumes the snapshot, returning the underlying document snapshots.
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

/// Typed wrapper around a `QuerySnapshot` using a data converter.
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

    /// Returns the underlying untyped snapshot.
    pub fn raw(&self) -> &QuerySnapshot {
        &self.base
    }

    /// Returns typed document snapshots for every document in the query result.
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
