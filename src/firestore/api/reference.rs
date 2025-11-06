use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use std::fmt::{Display, Formatter};
use std::sync::Arc;

use crate::firestore::error::{invalid_argument, FirestoreResult};
use crate::firestore::model::{DocumentKey, ResourcePath};

use super::converter::FirestoreDataConverter;
use super::database::Firestore;
use super::query::{ConvertedQuery, Query};

#[derive(Clone, Debug)]
pub struct CollectionReference {
    firestore: Firestore,
    path: ResourcePath,
}

impl CollectionReference {
    pub(crate) fn new(firestore: Firestore, path: ResourcePath) -> FirestoreResult<Self> {
        if path.len() % 2 == 0 {
            return Err(invalid_argument(
                "Collection references must point to a collection (odd number of segments)",
            ));
        }
        Ok(Self { firestore, path })
    }

    /// Returns the Firestore instance that created this collection reference.
    pub fn firestore(&self) -> &Firestore {
        &self.firestore
    }

    /// The full resource path of the collection (e.g. `rooms/eros/messages`).
    pub fn path(&self) -> &ResourcePath {
        &self.path
    }

    /// The last segment of the collection path.
    pub fn id(&self) -> &str {
        self.path
            .last_segment()
            .expect("Collection path always has id")
    }

    /// Returns the document that logically contains this collection, if any.
    pub fn parent(&self) -> Option<DocumentReference> {
        self.path.pop_last().and_then(|parent_path| {
            if parent_path.is_empty() || parent_path.len() % 2 != 0 {
                return None;
            }
            DocumentReference::new(self.firestore.clone(), parent_path).ok()
        })
    }

    /// Returns a reference to the document identified by `document_id`.
    ///
    /// When `document_id` is `None`, an auto-ID is generated.
    pub fn doc(&self, document_id: Option<&str>) -> FirestoreResult<DocumentReference> {
        let id = document_id
            .map(|id| id.to_string())
            .unwrap_or_else(generate_auto_id);
        if id.contains('/') {
            return Err(invalid_argument("Document ID cannot contain '/'."));
        }
        let path = self.path.child([id]);
        DocumentReference::new(self.firestore.clone(), path)
    }

    pub fn with_converter<C>(&self, converter: C) -> ConvertedCollectionReference<C>
    where
        C: FirestoreDataConverter,
    {
        ConvertedCollectionReference {
            inner: self.clone(),
            converter: Arc::new(converter),
        }
    }

    /// Creates a query that targets this collection.
    pub fn query(&self) -> Query {
        Query::new(self.firestore.clone(), self.path.clone())
            .expect("CollectionReference always points to a valid collection")
    }
}

impl Display for CollectionReference {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "CollectionReference({})", self.path.canonical_string())
    }
}

#[derive(Clone, Debug)]
pub struct DocumentReference {
    firestore: Firestore,
    key: DocumentKey,
}

impl DocumentReference {
    pub(crate) fn new(firestore: Firestore, path: ResourcePath) -> FirestoreResult<Self> {
        let key = DocumentKey::from_path(path)?;
        Ok(Self { firestore, key })
    }

    /// Returns the Firestore instance that created this document reference.
    pub fn firestore(&self) -> &Firestore {
        &self.firestore
    }

    /// The document identifier (the last segment of its path).
    pub fn id(&self) -> &str {
        self.key.id()
    }

    /// The full resource path to the document.
    pub fn path(&self) -> &ResourcePath {
        self.key.path()
    }

    /// The parent collection containing this document.
    pub fn parent(&self) -> CollectionReference {
        CollectionReference::new(self.firestore.clone(), self.key.collection_path())
            .expect("Document parent path is always a collection")
    }

    /// Returns a reference to a subcollection rooted at this document.
    pub fn collection(&self, path: &str) -> FirestoreResult<CollectionReference> {
        let sub_path = ResourcePath::from_string(path)?;
        let full_path = self.key.path().child(sub_path.as_vec().clone());
        CollectionReference::new(self.firestore.clone(), full_path)
    }

    /// Returns a typed document reference using the provided converter.
    pub fn with_converter<C>(&self, converter: C) -> ConvertedDocumentReference<C>
    where
        C: FirestoreDataConverter,
    {
        ConvertedDocumentReference::new(self.clone(), Arc::new(converter))
    }
}

impl Display for DocumentReference {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DocumentReference({})",
            self.key.path().canonical_string()
        )
    }
}

fn generate_auto_id() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .map(char::from)
        .take(20)
        .collect()
}

#[derive(Clone)]
pub struct ConvertedCollectionReference<C>
where
    C: FirestoreDataConverter,
{
    inner: CollectionReference,
    converter: Arc<C>,
}

impl<C> ConvertedCollectionReference<C>
where
    C: FirestoreDataConverter,
{
    /// Accesses the underlying Firestore instance.
    pub fn firestore(&self) -> &Firestore {
        self.inner.firestore()
    }

    /// Full resource path for the collection.
    pub fn path(&self) -> &ResourcePath {
        self.inner.path()
    }

    /// The collection identifier (last path segment).
    pub fn id(&self) -> &str {
        self.inner.id()
    }

    /// Returns a typed document reference within this collection.
    pub fn doc(&self, document_id: Option<&str>) -> FirestoreResult<ConvertedDocumentReference<C>> {
        let document = self.inner.doc(document_id)?;
        Ok(ConvertedDocumentReference::new(
            document,
            Arc::clone(&self.converter),
        ))
    }

    /// Provides access to the untyped collection reference.
    pub fn raw(&self) -> &CollectionReference {
        &self.inner
    }

    /// Creates a query for the underlying collection using this converter.
    pub fn query(&self) -> ConvertedQuery<C> {
        ConvertedQuery::new(self.inner.query(), Arc::clone(&self.converter))
    }
}

#[derive(Clone)]
pub struct ConvertedDocumentReference<C>
where
    C: FirestoreDataConverter,
{
    reference: DocumentReference,
    converter: Arc<C>,
}

impl<C> ConvertedDocumentReference<C>
where
    C: FirestoreDataConverter,
{
    fn new(reference: DocumentReference, converter: Arc<C>) -> Self {
        Self {
            reference,
            converter,
        }
    }

    /// Accesses the underlying Firestore instance.
    pub fn firestore(&self) -> &Firestore {
        self.reference.firestore()
    }

    /// The document identifier assigned to this reference.
    pub fn id(&self) -> &str {
        self.reference.id()
    }

    /// Full resource path for the document.
    pub fn path(&self) -> &ResourcePath {
        self.reference.path()
    }

    /// Returns the parent collection.
    pub fn parent(&self) -> CollectionReference {
        self.reference.parent()
    }

    /// Provides access to the untyped document reference.
    pub fn raw(&self) -> &DocumentReference {
        &self.reference
    }

    /// Clones the converter used to map data for this reference.
    pub fn converter(&self) -> Arc<C> {
        Arc::clone(&self.converter)
    }

    pub fn with_converter<D>(&self, converter: D) -> ConvertedDocumentReference<D>
    where
        D: FirestoreDataConverter,
    {
        ConvertedDocumentReference::new(self.reference.clone(), Arc::new(converter))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use crate::firestore::api::database::get_firestore;

    fn unique_settings() -> FirebaseAppSettings {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!(
                "firestore-ref-{}",
                COUNTER.fetch_add(1, Ordering::SeqCst)
            )),
            ..Default::default()
        }
    }

    async fn setup_firestore() -> Firestore {
        let options = FirebaseOptions {
            project_id: Some("test-project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .unwrap();
        let firestore = get_firestore(Some(app)).await.unwrap();
        Firestore::from_arc(firestore)
    }

    #[tokio::test]
    async fn collection_and_document_roundtrip() {
        let firestore = setup_firestore().await;
        let collection = firestore.collection("cities").unwrap();
        assert_eq!(collection.id(), "cities");
        let document = collection.doc(Some("sf")).unwrap();
        assert_eq!(document.id(), "sf");
        assert_eq!(document.parent().id(), "cities");
    }

    #[tokio::test]
    async fn auto_id_generation() {
        let firestore = setup_firestore().await;
        let collection = firestore.collection("cities").unwrap();
        let document = collection.doc(None).unwrap();
        assert_eq!(document.parent().id(), "cities");
        assert_eq!(document.id().len(), 20);
    }
}
