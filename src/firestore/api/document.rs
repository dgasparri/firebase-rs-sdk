use std::collections::BTreeMap;

use crate::firestore::api::operations::{self, SetOptions};
use crate::firestore::api::query::{
    ConvertedQuery, LimitType, Query, QuerySnapshot, TypedQuerySnapshot,
};
use crate::firestore::api::snapshot::{DocumentSnapshot, TypedDocumentSnapshot};
use crate::firestore::error::{internal_error, FirestoreResult};
use std::sync::Arc;

use crate::firestore::remote::datastore::{
    Datastore, HttpDatastore, InMemoryDatastore, TokenProviderArc,
};
use crate::firestore::value::FirestoreValue;

use super::{
    ConvertedCollectionReference, ConvertedDocumentReference, Firestore, FirestoreDataConverter,
};

#[derive(Clone)]
pub struct FirestoreClient {
    firestore: Firestore,
    datastore: Arc<dyn Datastore>,
}

impl FirestoreClient {
    /// Creates a client backed by the supplied datastore implementation.
    pub fn new(firestore: Firestore, datastore: Arc<dyn Datastore>) -> Self {
        Self {
            firestore,
            datastore,
        }
    }

    /// Returns a client that stores documents in memory only.
    ///
    /// Useful for tests or demos where persistence/network access is not
    /// required.
    pub fn with_in_memory(firestore: Firestore) -> Self {
        Self::new(firestore, Arc::new(InMemoryDatastore::new()))
    }

    /// Builds a client that talks to Firestore over the REST endpoints using
    /// anonymous credentials.
    pub fn with_http_datastore(firestore: Firestore) -> FirestoreResult<Self> {
        let datastore = HttpDatastore::from_database_id(firestore.database_id().clone())?;
        Ok(Self::new(firestore, Arc::new(datastore)))
    }

    /// Builds an HTTP-backed client that attaches the provided Auth/App Check
    /// providers to every request.
    ///
    /// Pass `None` for `app_check_provider` when App Check is not configured.
    pub fn with_http_datastore_authenticated(
        firestore: Firestore,
        auth_provider: TokenProviderArc,
        app_check_provider: Option<TokenProviderArc>,
    ) -> FirestoreResult<Self> {
        let mut builder = HttpDatastore::builder(firestore.database_id().clone())
            .with_auth_provider(auth_provider);

        if let Some(provider) = app_check_provider {
            builder = builder.with_app_check_provider(provider);
        }

        let datastore = builder.build()?;
        Ok(Self::new(firestore, Arc::new(datastore)))
    }

    /// Fetches the document located at `path`.
    ///
    /// Returns a snapshot that may or may not contain data depending on whether
    /// the document exists.
    pub async fn get_doc(&self, path: &str) -> FirestoreResult<DocumentSnapshot> {
        let key = operations::validate_document_path(path)?;
        self.datastore.get_document(&key).await
    }

    /// Writes the provided map of fields into the document at `path`.
    ///
    /// `options.merge == true` mirrors the JS API but is currently unsupported
    /// for the HTTP datastore.
    pub async fn set_doc(
        &self,
        path: &str,
        data: BTreeMap<String, FirestoreValue>,
        options: Option<SetOptions>,
    ) -> FirestoreResult<()> {
        let key = operations::validate_document_path(path)?;
        let encoded = operations::encode_document_data(data)?;
        let merge = options.unwrap_or_default().merge;
        self.datastore.set_document(&key, encoded, merge).await
    }

    /// Applies a partial update to the document located at `path`.
    ///
    /// This mirrors the behaviour of the JS `updateDoc` API by only touching the
    /// provided fields and requiring the document to exist.
    ///
    /// # Errors
    /// Returns `firestore/invalid-argument` if `data` is empty and
    /// `firestore/not-found` if the document does not exist.
    ///
    /// # Examples
    /// ```ignore
    /// client
    ///     .update_doc(
    ///         "cities/sf",
    ///         BTreeMap::from([
    ///             ("population".into(), FirestoreValue::from_integer(900_000)),
    ///         ]),
    ///     )
    ///     .await?;
    /// ```
    ///
    /// TypeScript reference: `updateDoc` in
    /// `packages/firestore/src/api/reference_impl.ts`.
    pub async fn update_doc(
        &self,
        path: &str,
        data: BTreeMap<String, FirestoreValue>,
    ) -> FirestoreResult<()> {
        let key = operations::validate_document_path(path)?;
        let (encoded, field_paths) = operations::encode_update_document_data(data)?;
        self.datastore
            .update_document(&key, encoded, field_paths)
            .await
    }

    /// Adds a new document to the collection located at `collection_path` and
    /// returns the resulting snapshot.
    pub async fn add_doc(
        &self,
        collection_path: &str,
        data: BTreeMap<String, FirestoreValue>,
    ) -> FirestoreResult<DocumentSnapshot> {
        let collection = self.firestore.collection(collection_path)?;
        let doc_ref = collection.doc(None)?;
        self.set_doc(doc_ref.path().canonical_string().as_str(), data, None)
            .await?;
        self.get_doc(doc_ref.path().canonical_string().as_str())
            .await
    }

    /// Reads a document using the converter attached to a typed reference.
    pub async fn get_doc_with_converter<C>(
        &self,
        reference: &ConvertedDocumentReference<C>,
    ) -> FirestoreResult<TypedDocumentSnapshot<C>>
    where
        C: FirestoreDataConverter,
    {
        let path = reference.path().canonical_string();
        let snapshot = self.get_doc(path.as_str()).await?;
        let converter = reference.converter();
        Ok(snapshot.into_typed(converter))
    }

    /// Updates a document referenced by a converted reference.
    ///
    /// Converters are intentionally ignored, matching the behaviour of the JS
    /// SDK.
    pub async fn update_doc_with_converter<C>(
        &self,
        reference: &ConvertedDocumentReference<C>,
        data: BTreeMap<String, FirestoreValue>,
    ) -> FirestoreResult<()>
    where
        C: FirestoreDataConverter,
    {
        let path = reference.path().canonical_string();
        self.update_doc(path.as_str(), data).await
    }

    /// Deletes the document located at `path`.
    ///
    /// Mirrors the JS `deleteDoc` API and succeeds even if the document does
    /// not exist.
    ///
    /// # Examples
    /// ```ignore
    /// client.delete_doc("cities/sf").await?;
    /// ```
    ///
    /// TypeScript reference: `deleteDoc` in
    /// `packages/firestore/src/api/reference_impl.ts`.
    pub async fn delete_doc(&self, path: &str) -> FirestoreResult<()> {
        let key = operations::validate_document_path(path)?;
        self.datastore.delete_document(&key).await
    }

    /// Deletes a document by converted reference.
    pub async fn delete_doc_with_converter<C>(
        &self,
        reference: &ConvertedDocumentReference<C>,
    ) -> FirestoreResult<()>
    where
        C: FirestoreDataConverter,
    {
        let path = reference.path().canonical_string();
        self.delete_doc(path.as_str()).await
    }

    /// Executes the provided query and returns its results.
    pub async fn get_docs(&self, query: &Query) -> FirestoreResult<QuerySnapshot> {
        self.ensure_same_database(query.firestore())?;
        let definition = query.definition();
        let mut documents = self.datastore.run_query(&definition).await?;
        if definition.limit_type() == LimitType::Last {
            documents.reverse();
        }
        Ok(QuerySnapshot::new(query.clone(), documents))
    }

    /// Executes a converted query, producing typed snapshots.
    pub async fn get_docs_with_converter<C>(
        &self,
        query: &ConvertedQuery<C>,
    ) -> FirestoreResult<TypedQuerySnapshot<C>>
    where
        C: FirestoreDataConverter,
    {
        let snapshot = self.get_docs(query.raw()).await?;
        Ok(TypedQuerySnapshot::new(snapshot, query.converter()))
    }

    /// Writes a typed model to the location referenced by `reference`.
    pub async fn set_doc_with_converter<C>(
        &self,
        reference: &ConvertedDocumentReference<C>,
        data: C::Model,
        options: Option<SetOptions>,
    ) -> FirestoreResult<()>
    where
        C: FirestoreDataConverter,
    {
        let converter = reference.converter();
        let map = converter.to_map(&data)?;
        let path = reference.path().canonical_string();
        self.set_doc(path.as_str(), map, options).await
    }

    /// Creates a document with auto-generated ID using the provided converter.
    pub async fn add_doc_with_converter<C>(
        &self,
        collection: &ConvertedCollectionReference<C>,
        data: C::Model,
    ) -> FirestoreResult<TypedDocumentSnapshot<C>>
    where
        C: FirestoreDataConverter,
    {
        let doc_ref = collection.doc(None)?;
        let converter = doc_ref.converter();
        let map = converter.to_map(&data)?;
        let path = doc_ref.path().canonical_string();
        self.set_doc(path.as_str(), map, None).await?;
        let snapshot = self.get_doc(path.as_str()).await?;
        Ok(snapshot.into_typed(converter))
    }

    fn ensure_same_database(&self, firestore: &Firestore) -> FirestoreResult<()> {
        if self.firestore.database_id() != firestore.database_id() {
            return Err(internal_error(
                "Query targets a different Firestore instance than this client",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::api::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use crate::firestore::api::get_firestore;
    use crate::firestore::model::FieldPath;
    use crate::firestore::value::MapValue;
    use crate::firestore::value::ValueKind;

    fn unique_settings() -> FirebaseAppSettings {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!(
                "firestore-doc-{}",
                COUNTER.fetch_add(1, Ordering::SeqCst)
            )),
            ..Default::default()
        }
    }

    async fn build_client() -> FirestoreClient {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .unwrap();
        let firestore = get_firestore(Some(app)).await.unwrap();
        FirestoreClient::with_in_memory(Firestore::from_arc(firestore))
    }

    #[tokio::test]
    async fn set_and_get_document() {
        let client = build_client().await;
        let mut data = BTreeMap::new();
        data.insert("name".to_string(), FirestoreValue::from_string("Ada"));
        client
            .set_doc("cities/sf", data.clone(), None)
            .await
            .expect("set doc");
        let snapshot = client.get_doc("cities/sf").await.expect("get doc");
        assert!(snapshot.exists());
        assert_eq!(
            snapshot.data().unwrap().get("name"),
            Some(&FirestoreValue::from_string("Ada"))
        );
    }

    #[tokio::test]
    async fn update_document_merges_fields() {
        let client = build_client().await;
        let mut initial = BTreeMap::new();
        initial.insert("name".to_string(), FirestoreValue::from_string("Ada"));
        let mut stats = BTreeMap::new();
        stats.insert("visits".to_string(), FirestoreValue::from_integer(1));
        stats.insert("likes".to_string(), FirestoreValue::from_integer(5));
        initial.insert("stats".to_string(), FirestoreValue::from_map(stats));
        client
            .set_doc("cities/sf", initial, None)
            .await
            .expect("set doc");

        let mut update = BTreeMap::new();
        let mut stats_update = BTreeMap::new();
        stats_update.insert("visits".to_string(), FirestoreValue::from_integer(2));
        stats_update.insert("shares".to_string(), FirestoreValue::from_integer(9));
        update.insert("stats".to_string(), FirestoreValue::from_map(stats_update));
        update.insert(
            "state".to_string(),
            FirestoreValue::from_string("California"),
        );

        client
            .update_doc("cities/sf", update)
            .await
            .expect("update doc");

        let snapshot = client.get_doc("cities/sf").await.expect("get doc");
        let data = snapshot.data().expect("data");
        assert_eq!(
            data.get("state"),
            Some(&FirestoreValue::from_string("California"))
        );
        let stats_value = data.get("stats").expect("stats present");
        match stats_value.kind() {
            ValueKind::Map(map) => {
                assert_eq!(
                    map.fields().get("visits"),
                    Some(&FirestoreValue::from_integer(2))
                );
                assert_eq!(
                    map.fields().get("likes"),
                    Some(&FirestoreValue::from_integer(5))
                );
                assert_eq!(
                    map.fields().get("shares"),
                    Some(&FirestoreValue::from_integer(9))
                );
            }
            _ => panic!("expected map"),
        }
    }

    #[tokio::test]
    async fn update_document_requires_existing() {
        let client = build_client().await;
        let mut update = BTreeMap::new();
        update.insert("name".to_string(), FirestoreValue::from_string("Ada"));
        let err = client
            .update_doc("cities/unknown", update)
            .await
            .expect_err("missing doc");
        assert_eq!(err.code_str(), "firestore/not-found");
    }

    #[tokio::test]
    async fn delete_document_clears_state() {
        let client = build_client().await;
        let mut data = BTreeMap::new();
        data.insert("name".to_string(), FirestoreValue::from_string("Ada"));
        client
            .set_doc("cities/sf", data, None)
            .await
            .expect("set doc");
        client.delete_doc("cities/sf").await.expect("delete doc");
        let snapshot = client.get_doc("cities/sf").await.expect("get doc");
        assert!(!snapshot.exists());
    }

    #[tokio::test]
    async fn delete_missing_document_is_noop() {
        let client = build_client().await;
        client
            .delete_doc("cities/non-existent")
            .await
            .expect("delete missing");
    }

    #[tokio::test]
    async fn query_returns_collection_documents() {
        let client = build_client().await;
        client
            .set_doc(
                "cities/sf",
                BTreeMap::from([("name".into(), FirestoreValue::from_string("San Francisco"))]),
                None,
            )
            .await
            .unwrap();
        client
            .set_doc(
                "cities/la",
                BTreeMap::from([("name".into(), FirestoreValue::from_string("Los Angeles"))]),
                None,
            )
            .await
            .unwrap();

        let collection = client.firestore.collection("cities").unwrap();
        let query = collection.query();
        let snapshot = client.get_docs(&query).await.expect("query");

        assert_eq!(snapshot.len(), 2);
        let ids: Vec<_> = snapshot
            .documents()
            .iter()
            .map(|doc| doc.id().to_string())
            .collect();
        assert_eq!(ids, vec!["la", "sf"]);
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Person {
        first: String,
        last: String,
    }

    #[derive(Clone, Debug)]
    struct PersonConverter;

    impl super::FirestoreDataConverter for PersonConverter {
        type Model = Person;

        fn to_map(&self, value: &Self::Model) -> FirestoreResult<BTreeMap<String, FirestoreValue>> {
            let mut map = BTreeMap::new();
            map.insert(
                "first".to_string(),
                FirestoreValue::from_string(&value.first),
            );
            map.insert("last".to_string(), FirestoreValue::from_string(&value.last));
            Ok(map)
        }

        fn from_map(&self, value: &MapValue) -> FirestoreResult<Self::Model> {
            let first = match value.fields().get("first").and_then(|v| match v.kind() {
                ValueKind::String(s) => Some(s.clone()),
                _ => None,
            }) {
                Some(name) => name,
                None => {
                    return Err(crate::firestore::error::invalid_argument(
                        "missing first field",
                    ))
                }
            };
            let last = match value.fields().get("last").and_then(|v| match v.kind() {
                ValueKind::String(s) => Some(s.clone()),
                _ => None,
            }) {
                Some(name) => name,
                None => {
                    return Err(crate::firestore::error::invalid_argument(
                        "missing last field",
                    ))
                }
            };
            Ok(Person { first, last })
        }
    }

    #[tokio::test]
    async fn typed_set_and_get_document() {
        let client = build_client().await;
        let collection = client.firestore.collection("people").unwrap();
        let converted = collection.with_converter(PersonConverter);
        let doc_ref = converted.doc(Some("ada")).unwrap();

        let person = Person {
            first: "Ada".into(),
            last: "Lovelace".into(),
        };

        client
            .set_doc_with_converter(&doc_ref, person.clone(), None)
            .await
            .expect("typed set");

        let snapshot = client
            .get_doc_with_converter(&doc_ref)
            .await
            .expect("typed get");
        assert!(snapshot.exists());
        assert!(snapshot.from_cache());
        assert!(!snapshot.has_pending_writes());

        let decoded = snapshot.data().expect("converter result").unwrap();
        assert_eq!(decoded, person);
    }

    #[tokio::test]
    async fn typed_query_returns_converted_results() {
        let client = build_client().await;
        let collection = client.firestore.collection("people").unwrap();
        let converted = collection.with_converter(PersonConverter);

        let doc_ref = converted.doc(Some("ada")).unwrap();
        let ada = Person {
            first: "Ada".into(),
            last: "Lovelace".into(),
        };
        client
            .set_doc_with_converter(&doc_ref, ada.clone(), None)
            .await
            .expect("set typed doc");

        let query = converted.query();
        let snapshot = client
            .get_docs_with_converter(&query)
            .await
            .expect("converted query");

        let docs = snapshot.documents();
        assert_eq!(docs.len(), 1);
        let decoded = docs[0].data().expect("converter data").unwrap();
        assert_eq!(decoded, ada);
    }

    #[tokio::test]
    async fn query_with_filters_and_limit() {
        use crate::firestore::api::query::{FilterOperator, OrderDirection};

        let client = build_client().await;
        let collection = client.firestore.collection("cities").unwrap();

        let mut sf = BTreeMap::new();
        sf.insert("name".into(), FirestoreValue::from_string("San Francisco"));
        sf.insert("state".into(), FirestoreValue::from_string("California"));
        sf.insert("population".into(), FirestoreValue::from_integer(860_000));
        client
            .set_doc("cities/sf", sf, None)
            .await
            .expect("insert sf");

        let mut la = BTreeMap::new();
        la.insert("name".into(), FirestoreValue::from_string("Los Angeles"));
        la.insert("state".into(), FirestoreValue::from_string("California"));
        la.insert("population".into(), FirestoreValue::from_integer(3_980_000));
        client
            .set_doc("cities/la", la, None)
            .await
            .expect("insert la");

        let query = collection
            .query()
            .where_field(
                FieldPath::from_dot_separated("state").unwrap(),
                FilterOperator::Equal,
                FirestoreValue::from_string("California"),
            )
            .unwrap()
            .order_by(
                FieldPath::from_dot_separated("population").unwrap(),
                OrderDirection::Descending,
            )
            .unwrap()
            .limit(1)
            .unwrap();

        let snapshot = client.get_docs(&query).await.expect("filtered query");
        assert_eq!(snapshot.len(), 1);
        let doc = &snapshot.documents()[0];
        assert_eq!(doc.id(), "la");
    }
}
