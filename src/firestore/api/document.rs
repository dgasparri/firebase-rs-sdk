use std::collections::BTreeMap;

use crate::firestore::api::aggregate::{AggregateField, AggregateQuerySnapshot, AggregateSpec};
use crate::firestore::api::operations::{self, SetOptions};
use crate::firestore::api::query::{
    compute_doc_changes, ConvertedQuery, LimitType, Query, QuerySnapshot, QuerySnapshotMetadata, TypedQuerySnapshot,
};
use crate::firestore::api::snapshot::{DocumentSnapshot, TypedDocumentSnapshot};
use crate::firestore::error::{internal_error, invalid_argument, FirestoreResult};
use std::sync::Arc;

use crate::firestore::remote::datastore::{Datastore, HttpDatastore, InMemoryDatastore, TokenProviderArc};
use crate::firestore::value::FirestoreValue;

use super::write_batch::WriteBatch;
use super::{
    converter::FirestoreDataConverter,
    database::Firestore,
    reference::{ConvertedCollectionReference, ConvertedDocumentReference},
};

const COUNT_ALIAS: &str = "count";

#[derive(Clone)]
pub struct FirestoreClient {
    firestore: Firestore,
    datastore: Arc<dyn Datastore>,
}

impl FirestoreClient {
    /// Creates a client backed by the supplied datastore implementation.
    pub fn new(firestore: Firestore, datastore: Arc<dyn Datastore>) -> Self {
        Self { firestore, datastore }
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
        let mut builder = HttpDatastore::builder(firestore.database_id().clone()).with_auth_provider(auth_provider);

        if let Some(provider) = app_check_provider {
            builder = builder.with_app_check_provider(provider);
        }

        let datastore = builder.build()?;
        Ok(Self::new(firestore, Arc::new(datastore)))
    }

    /// Creates a new write batch that targets the same Firestore instance as this client.
    ///
    /// TypeScript reference: `writeBatch(firestore)` in
    /// `packages/firestore/src/lite-api/write_batch.ts`.
    pub fn batch(&self) -> WriteBatch {
        WriteBatch::new(self.firestore.clone(), Arc::clone(&self.datastore))
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
        let options = options.unwrap_or_default();
        let encoded = operations::encode_set_data(data, &options)?;
        self.datastore
            .set_document(&key, encoded.map, encoded.mask, encoded.transforms)
            .await
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
    /// ```rust,no_run
    /// # use std::sync::Arc;
    /// # use firebase_rs_sdk::doctest_support::{firestore::get_mock_client};
    /// # use firebase_rs_sdk::firestore::FirestoreResult;
    /// # async fn run() -> FirestoreResult<()> {
    /// # let client = get_mock_client(None).await;
    /// use std::collections::BTreeMap;
    ///
    /// use firebase_rs_sdk::firestore::FirestoreValue;
    ///
    /// client
    ///     .update_doc(
    ///         "cities/sf",
    ///         BTreeMap::from([
    ///             ("population".into(), FirestoreValue::from_integer(900_000)),
    ///         ]),
    ///     )
    ///     .await?;
    /// # Ok(()) }
    /// ```
    ///
    /// TypeScript reference: `updateDoc` in
    /// `packages/firestore/src/api/reference_impl.ts`.
    pub async fn update_doc(&self, path: &str, data: BTreeMap<String, FirestoreValue>) -> FirestoreResult<()> {
        let key = operations::validate_document_path(path)?;
        let encoded = operations::encode_update_document_data(data)?;
        self.datastore
            .update_document(&key, encoded.map, encoded.field_paths, encoded.transforms)
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
        self.get_doc(doc_ref.path().canonical_string().as_str()).await
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
    /// ```rust,no_run
    /// # use std::sync::Arc;
    /// # use firebase_rs_sdk::doctest_support::{firestore::get_mock_client};
    /// # use firebase_rs_sdk::firestore::FirestoreResult;
    /// # async fn run() -> FirestoreResult<()> {
    /// # let client = get_mock_client(None).await;
    /// client.delete_doc("cities/sf").await?;
    /// # Ok(()) }
    /// ```
    ///
    /// TypeScript reference: `deleteDoc` in
    /// `packages/firestore/src/api/reference_impl.ts`.
    pub async fn delete_doc(&self, path: &str) -> FirestoreResult<()> {
        let key = operations::validate_document_path(path)?;
        self.datastore.delete_document(&key).await
    }

    /// Deletes a document by converted reference.
    pub async fn delete_doc_with_converter<C>(&self, reference: &ConvertedDocumentReference<C>) -> FirestoreResult<()>
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
        let metadata = QuerySnapshotMetadata::new(false, false, false, None, None);
        let doc_changes = compute_doc_changes(None, &documents);
        Ok(QuerySnapshot::new(query.clone(), documents, metadata, doc_changes))
    }

    /// Executes a converted query, producing typed snapshots.
    pub async fn get_docs_with_converter<C>(&self, query: &ConvertedQuery<C>) -> FirestoreResult<TypedQuerySnapshot<C>>
    where
        C: FirestoreDataConverter,
    {
        let snapshot = self.get_docs(query.raw()).await?;
        Ok(TypedQuerySnapshot::new(snapshot, query.converter()))
    }

    /// Executes the provided aggregate specification against `query`.
    ///
    /// Mirrors the modular JS `getAggregate(query, spec)` helper from
    /// `packages/firestore/src/lite-api/aggregate.ts`.
    pub async fn get_aggregate(&self, query: &Query, spec: AggregateSpec) -> FirestoreResult<AggregateQuerySnapshot> {
        if spec.is_empty() {
            return Err(invalid_argument("Aggregate spec must contain at least one field"));
        }
        self.ensure_same_database(query.firestore())?;
        let definition = query.definition();
        let aggregates = spec.definitions();
        let data = self.datastore.run_aggregate(&definition, &aggregates).await?;
        Ok(AggregateQuerySnapshot::new(query.clone(), spec, data))
    }

    /// Runs an aggregate query against a typed query definition.
    pub async fn get_aggregate_with_converter<C>(
        &self,
        query: &ConvertedQuery<C>,
        spec: AggregateSpec,
    ) -> FirestoreResult<AggregateQuerySnapshot>
    where
        C: FirestoreDataConverter,
    {
        self.get_aggregate(query.raw(), spec).await
    }

    /// Counts the number of documents that match `query` without downloading them.
    ///
    /// Mirrors the modular JS `getCount(query)` helper from
    /// `packages/firestore/src/lite-api/aggregate.ts`.
    pub async fn get_count(&self, query: &Query) -> FirestoreResult<AggregateQuerySnapshot> {
        let mut spec = AggregateSpec::new();
        spec.insert(COUNT_ALIAS, AggregateField::count())?;
        self.get_aggregate(query, spec).await
    }

    /// Counts matching documents for a converted query.
    pub async fn get_count_with_converter<C>(
        &self,
        query: &ConvertedQuery<C>,
    ) -> FirestoreResult<AggregateQuerySnapshot>
    where
        C: FirestoreDataConverter,
    {
        let mut spec = AggregateSpec::new();
        spec.insert(COUNT_ALIAS, AggregateField::count())?;
        self.get_aggregate(query.raw(), spec).await
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
            return Err(internal_error("Query targets a different Firestore instance than this client"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use crate::firestore::api::aggregate::{AggregateField, AggregateSpec};
    use crate::firestore::api::database::get_firestore;
    use crate::firestore::model::FieldPath;
    use crate::firestore::value::MapValue;
    use crate::firestore::value::ValueKind;
    use crate::firestore::FilterOperator;

    fn unique_settings() -> FirebaseAppSettings {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!("firestore-doc-{}", COUNTER.fetch_add(1, Ordering::SeqCst))),
            ..Default::default()
        }
    }

    async fn build_client_with_firestore() -> (FirestoreClient, Firestore) {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).await.unwrap();
        let firestore = get_firestore(Some(app)).await.unwrap();
        let firestore = Firestore::from_arc(firestore);
        let client = FirestoreClient::with_in_memory(firestore.clone());
        (client, firestore)
    }

    async fn build_client() -> FirestoreClient {
        build_client_with_firestore().await.0
    }

    #[tokio::test]
    async fn set_and_get_document() {
        let client = build_client().await;
        let mut data = BTreeMap::new();
        data.insert("name".to_string(), FirestoreValue::from_string("Ada"));
        client.set_doc("cities/sf", data.clone(), None).await.expect("set doc");
        let snapshot = client.get_doc("cities/sf").await.expect("get doc");
        assert!(snapshot.exists());
        assert_eq!(snapshot.data().unwrap().get("name"), Some(&FirestoreValue::from_string("Ada")));
    }

    #[tokio::test]
    async fn set_doc_with_merge_preserves_existing_fields() {
        let client = build_client().await;
        let mut initial = BTreeMap::new();
        initial.insert("name".to_string(), FirestoreValue::from_string("San Francisco"));
        let mut stats = BTreeMap::new();
        stats.insert("population".to_string(), FirestoreValue::from_integer(100));
        initial.insert("stats".to_string(), FirestoreValue::from_map(stats));
        client.set_doc("cities/sf", initial, None).await.expect("initial set");

        let mut merge_data = BTreeMap::new();
        let mut stats_update = BTreeMap::new();
        stats_update.insert("population".to_string(), FirestoreValue::from_integer(150));
        merge_data.insert("stats".to_string(), FirestoreValue::from_map(stats_update));
        client
            .set_doc("cities/sf", merge_data, Some(SetOptions::merge_all()))
            .await
            .expect("merge set");

        let snapshot = client.get_doc("cities/sf").await.expect("get doc");
        let data = snapshot.data().expect("data");
        assert_eq!(data.get("name"), Some(&FirestoreValue::from_string("San Francisco")));
        let stats_map = match data.get("stats").unwrap().kind() {
            ValueKind::Map(map) => map,
            _ => panic!("expected stats map"),
        };
        assert_eq!(stats_map.fields().get("population"), Some(&FirestoreValue::from_integer(150)));
    }

    #[tokio::test]
    async fn merge_fields_only_updates_requested_paths() {
        let client = build_client().await;
        let mut initial = BTreeMap::new();
        let mut stats = BTreeMap::new();
        stats.insert("wins".to_string(), FirestoreValue::from_integer(3));
        stats.insert("losses".to_string(), FirestoreValue::from_integer(5));
        initial.insert("stats".to_string(), FirestoreValue::from_map(stats));
        client
            .set_doc("teams/giants", initial, None)
            .await
            .expect("initial set");

        let mut update = BTreeMap::new();
        let mut stats_update = BTreeMap::new();
        stats_update.insert("wins".to_string(), FirestoreValue::from_integer(4));
        stats_update.insert("losses".to_string(), FirestoreValue::from_integer(6));
        update.insert("stats".to_string(), FirestoreValue::from_map(stats_update));

        let options = SetOptions::merge_fields(vec![FieldPath::from_dot_separated("stats.wins").unwrap()]).unwrap();
        client
            .set_doc("teams/giants", update, Some(options))
            .await
            .expect("merge fields");

        let snapshot = client.get_doc("teams/giants").await.expect("get doc");
        let stats = match snapshot.data().expect("data").get("stats").expect("stats").kind() {
            ValueKind::Map(map) => map,
            _ => panic!("expected map"),
        };
        assert_eq!(stats.fields().get("wins"), Some(&FirestoreValue::from_integer(4)));
        assert_eq!(stats.fields().get("losses"), Some(&FirestoreValue::from_integer(5)));
    }

    #[tokio::test]
    async fn array_contains_query_returns_expected_documents() {
        let (client, firestore) = build_client_with_firestore().await;
        let collection = firestore.collection("places").unwrap();
        let mut data = BTreeMap::new();
        data.insert(
            "tags".to_string(),
            FirestoreValue::from_array(vec![
                FirestoreValue::from_string("coastal"),
                FirestoreValue::from_string("tourism"),
            ]),
        );
        client.set_doc("places/sf", data, None).await.expect("set doc");

        let query = collection
            .query()
            .where_field(
                FieldPath::from_dot_separated("tags").unwrap(),
                FilterOperator::ArrayContains,
                FirestoreValue::from_string("coastal"),
            )
            .unwrap();

        let snapshot = client.get_docs(&query).await.expect("query");
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot.documents()[0].id(), "sf");
    }

    #[tokio::test]
    async fn in_filter_matches_documents() {
        let (client, firestore) = build_client_with_firestore().await;
        client
            .set_doc(
                "cities/la",
                BTreeMap::from([("region".to_string(), FirestoreValue::from_string("west"))]),
                None,
            )
            .await
            .expect("set la");
        client
            .set_doc(
                "cities/nyc",
                BTreeMap::from([("region".to_string(), FirestoreValue::from_string("east"))]),
                None,
            )
            .await
            .expect("set nyc");

        let values = FirestoreValue::from_array(vec![
            FirestoreValue::from_string("west"),
            FirestoreValue::from_string("south"),
        ]);

        let query = firestore
            .collection("cities")
            .unwrap()
            .query()
            .where_field(FieldPath::from_dot_separated("region").unwrap(), FilterOperator::In, values)
            .unwrap();

        let snapshot = client.get_docs(&query).await.expect("query");
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot.documents()[0].id(), "la");
    }

    #[tokio::test]
    async fn server_timestamp_transform_sets_value() {
        let (client, _) = build_client_with_firestore().await;
        client
            .set_doc("cities/sf", BTreeMap::new(), None)
            .await
            .expect("seed doc");

        client
            .update_doc(
                "cities/sf",
                BTreeMap::from([("updated_at".to_string(), FirestoreValue::server_timestamp())]),
            )
            .await
            .expect("server timestamp");

        let snapshot = client.get_doc("cities/sf").await.expect("get doc");
        let value = snapshot.data().unwrap().get("updated_at").unwrap();
        match value.kind() {
            ValueKind::Timestamp(_) => {}
            other => panic!("expected timestamp transform, found {other:?}"),
        }
    }

    #[tokio::test]
    async fn array_union_transform_appends_unique_elements() {
        let (client, _) = build_client_with_firestore().await;
        let mut initial = BTreeMap::new();
        initial.insert(
            "tags".to_string(),
            FirestoreValue::from_array(vec![FirestoreValue::from_string("coastal")]),
        );
        client.set_doc("places/sf", initial, None).await.expect("seed doc");

        client
            .update_doc(
                "places/sf",
                BTreeMap::from([(
                    "tags".to_string(),
                    FirestoreValue::array_union(vec![
                        FirestoreValue::from_string("coastal"),
                        FirestoreValue::from_string("tourism"),
                    ]),
                )]),
            )
            .await
            .expect("array union");

        let snapshot = client.get_doc("places/sf").await.expect("get doc");
        let tags = snapshot.data().unwrap().get("tags").unwrap();
        let values = match tags.kind() {
            ValueKind::Array(array) => array
                .values()
                .iter()
                .map(|value| match value.kind() {
                    ValueKind::String(text) => text.clone(),
                    other => panic!("expected string tag, found {other:?}"),
                })
                .collect::<Vec<_>>(),
            other => panic!("expected array value, found {other:?}"),
        };
        assert_eq!(values, vec!["coastal", "tourism"]);
    }

    #[tokio::test]
    async fn array_remove_transform_removes_elements() {
        let (client, _) = build_client_with_firestore().await;
        let mut initial = BTreeMap::new();
        initial.insert(
            "tags".to_string(),
            FirestoreValue::from_array(vec![
                FirestoreValue::from_string("coastal"),
                FirestoreValue::from_string("tourism"),
            ]),
        );
        client.set_doc("places/sf", initial, None).await.expect("seed doc");

        client
            .update_doc(
                "places/sf",
                BTreeMap::from([(
                    "tags".to_string(),
                    FirestoreValue::array_remove(vec![FirestoreValue::from_string("coastal")]),
                )]),
            )
            .await
            .expect("array remove");

        let snapshot = client.get_doc("places/sf").await.expect("get doc");
        let tags = snapshot.data().unwrap().get("tags").unwrap();
        let values = match tags.kind() {
            ValueKind::Array(array) => array
                .values()
                .iter()
                .map(|value| match value.kind() {
                    ValueKind::String(text) => text.clone(),
                    other => panic!("expected string tag, found {other:?}"),
                })
                .collect::<Vec<_>>(),
            other => panic!("expected array value, found {other:?}"),
        };
        assert_eq!(values, vec!["tourism"]);
    }

    #[tokio::test]
    async fn numeric_increment_transform_updates_value() {
        let (client, _) = build_client_with_firestore().await;
        client
            .set_doc(
                "stats/snapshot",
                BTreeMap::from([("counter".to_string(), FirestoreValue::from_integer(1))]),
                None,
            )
            .await
            .expect("seed doc");

        client
            .update_doc(
                "stats/snapshot",
                BTreeMap::from([(
                    "counter".to_string(),
                    FirestoreValue::numeric_increment(FirestoreValue::from_integer(5)),
                )]),
            )
            .await
            .expect("increment");

        let snapshot = client.get_doc("stats/snapshot").await.expect("get doc");
        let counter = snapshot.data().unwrap().get("counter").unwrap();
        match counter.kind() {
            ValueKind::Integer(value) => assert_eq!(*value, 6),
            other => panic!("expected integer counter, found {other:?}"),
        }
    }

    #[tokio::test]
    async fn write_batch_applies_all_operations() {
        let (client, firestore) = build_client_with_firestore().await;
        let cities = firestore.collection("cities").unwrap();
        let denver = cities.doc(Some("denver")).unwrap();
        let la = cities.doc(Some("la")).unwrap();
        let abandoned = cities.doc(Some("ghost")).unwrap();

        let mut batch = client.batch();
        batch
            .set(
                &denver,
                BTreeMap::from([("population".to_string(), FirestoreValue::from_integer(700_000))]),
                None,
            )
            .unwrap();
        batch
            .update(
                &denver,
                BTreeMap::from([("state".to_string(), FirestoreValue::from_string("CO"))]),
            )
            .unwrap();
        batch
            .set(
                &la,
                BTreeMap::from([("population".to_string(), FirestoreValue::from_integer(3_000_000))]),
                None,
            )
            .unwrap();
        batch
            .set(
                &abandoned,
                BTreeMap::from([("should_delete".to_string(), FirestoreValue::from_bool(true))]),
                None,
            )
            .unwrap();
        batch.delete(&abandoned).unwrap();

        batch.commit().await.expect("commit");

        let denver_snapshot = client.get_doc("cities/denver").await.unwrap();
        let denver_data = denver_snapshot.data().unwrap();
        assert_eq!(denver_data.get("state"), Some(&FirestoreValue::from_string("CO")));
        assert_eq!(denver_data.get("population"), Some(&FirestoreValue::from_integer(700_000)));

        let la_snapshot = client.get_doc("cities/la").await.unwrap();
        assert!(la_snapshot.exists());

        let deleted_snapshot = client.get_doc("cities/ghost").await.unwrap();
        assert!(!deleted_snapshot.exists());
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
        client.set_doc("cities/sf", initial, None).await.expect("set doc");

        let mut update = BTreeMap::new();
        let mut stats_update = BTreeMap::new();
        stats_update.insert("visits".to_string(), FirestoreValue::from_integer(2));
        stats_update.insert("shares".to_string(), FirestoreValue::from_integer(9));
        update.insert("stats".to_string(), FirestoreValue::from_map(stats_update));
        update.insert("state".to_string(), FirestoreValue::from_string("California"));

        client.update_doc("cities/sf", update).await.expect("update doc");

        let snapshot = client.get_doc("cities/sf").await.expect("get doc");
        let data = snapshot.data().expect("data");
        assert_eq!(data.get("state"), Some(&FirestoreValue::from_string("California")));
        let stats_value = data.get("stats").expect("stats present");
        match stats_value.kind() {
            ValueKind::Map(map) => {
                assert_eq!(map.fields().get("visits"), Some(&FirestoreValue::from_integer(2)));
                assert_eq!(map.fields().get("likes"), Some(&FirestoreValue::from_integer(5)));
                assert_eq!(map.fields().get("shares"), Some(&FirestoreValue::from_integer(9)));
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
        client.set_doc("cities/sf", data, None).await.expect("set doc");
        client.delete_doc("cities/sf").await.expect("delete doc");
        let snapshot = client.get_doc("cities/sf").await.expect("get doc");
        assert!(!snapshot.exists());
    }

    #[tokio::test]
    async fn delete_missing_document_is_noop() {
        let client = build_client().await;
        client.delete_doc("cities/non-existent").await.expect("delete missing");
    }

    #[tokio::test]
    async fn query_returns_collection_documents() {
        let (client, firestore) = build_client_with_firestore().await;
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

        let collection = firestore.collection("cities").unwrap();
        let query = collection.query();
        let snapshot = client.get_docs(&query).await.expect("query");

        assert_eq!(snapshot.len(), 2);
        let ids: Vec<_> = snapshot.documents().iter().map(|doc| doc.id().to_string()).collect();
        assert_eq!(ids, vec!["la", "sf"]);
    }

    #[tokio::test]
    async fn aggregate_count_returns_total_documents() {
        let (client, firestore) = build_client_with_firestore().await;
        client
            .set_doc(
                "cities/sf",
                BTreeMap::from([("population".into(), FirestoreValue::from_integer(100))]),
                None,
            )
            .await
            .unwrap();
        client
            .set_doc(
                "cities/la",
                BTreeMap::from([("population".into(), FirestoreValue::from_integer(50))]),
                None,
            )
            .await
            .unwrap();

        let query = firestore.collection("cities").unwrap().query();
        let snapshot = client.get_count(&query).await.expect("aggregate count");
        let count = snapshot.count(COUNT_ALIAS).expect("count result").unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn aggregate_sum_and_average_compute_numeric_fields() {
        let (client, firestore) = build_client_with_firestore().await;
        client
            .set_doc(
                "cities/sf",
                BTreeMap::from([("population".into(), FirestoreValue::from_integer(100))]),
                None,
            )
            .await
            .unwrap();
        client
            .set_doc(
                "cities/la",
                BTreeMap::from([("population".into(), FirestoreValue::from_integer(50))]),
                None,
            )
            .await
            .unwrap();

        let query = firestore.collection("cities").unwrap().query();
        let mut spec = AggregateSpec::new();
        spec.insert("total_population", AggregateField::sum("population").unwrap())
            .unwrap();
        spec.insert("average_population", AggregateField::average("population").unwrap())
            .unwrap();

        let snapshot = client.get_aggregate(&query, spec).await.expect("aggregate query");

        let total = snapshot.get("total_population").expect("total value");
        match total.kind() {
            ValueKind::Integer(value) => assert_eq!(*value, 150),
            other => panic!("expected integer total, found {other:?}"),
        }

        let average = snapshot.get("average_population").expect("average value");
        match average.kind() {
            ValueKind::Double(value) => assert!((*value - 75.0).abs() < f64::EPSILON),
            other => panic!("expected double average, found {other:?}"),
        }
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
            map.insert("first".to_string(), FirestoreValue::from_string(&value.first));
            map.insert("last".to_string(), FirestoreValue::from_string(&value.last));
            Ok(map)
        }

        fn from_map(&self, value: &MapValue) -> FirestoreResult<Self::Model> {
            let first = match value.fields().get("first").and_then(|v| match v.kind() {
                ValueKind::String(s) => Some(s.clone()),
                _ => None,
            }) {
                Some(name) => name,
                None => return Err(crate::firestore::error::invalid_argument("missing first field")),
            };
            let last = match value.fields().get("last").and_then(|v| match v.kind() {
                ValueKind::String(s) => Some(s.clone()),
                _ => None,
            }) {
                Some(name) => name,
                None => return Err(crate::firestore::error::invalid_argument("missing last field")),
            };
            Ok(Person { first, last })
        }
    }

    #[tokio::test]
    async fn typed_set_and_get_document() {
        let (client, firestore) = build_client_with_firestore().await;
        let collection = firestore.collection("people").unwrap();
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

        let snapshot = client.get_doc_with_converter(&doc_ref).await.expect("typed get");
        assert!(snapshot.exists());
        assert!(snapshot.from_cache());
        assert!(!snapshot.has_pending_writes());

        let decoded = snapshot.data().expect("converter result").unwrap();
        assert_eq!(decoded, person);
    }

    #[tokio::test]
    async fn typed_query_returns_converted_results() {
        let (client, firestore) = build_client_with_firestore().await;
        let collection = firestore.collection("people").unwrap();
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
        let snapshot = client.get_docs_with_converter(&query).await.expect("converted query");

        let docs = snapshot.documents();
        assert_eq!(docs.len(), 1);
        let decoded = docs[0].data().expect("converter data").unwrap();
        assert_eq!(decoded, ada);
    }

    #[tokio::test]
    async fn query_with_filters_and_limit() {
        use crate::firestore::api::query::{FilterOperator, OrderDirection};

        let (client, firestore) = build_client_with_firestore().await;
        let collection = firestore.collection("cities").unwrap();

        let mut sf = BTreeMap::new();
        sf.insert("name".into(), FirestoreValue::from_string("San Francisco"));
        sf.insert("state".into(), FirestoreValue::from_string("California"));
        sf.insert("population".into(), FirestoreValue::from_integer(860_000));
        client.set_doc("cities/sf", sf, None).await.expect("insert sf");

        let mut la = BTreeMap::new();
        la.insert("name".into(), FirestoreValue::from_string("Los Angeles"));
        la.insert("state".into(), FirestoreValue::from_string("California"));
        la.insert("population".into(), FirestoreValue::from_integer(3_980_000));
        client.set_doc("cities/la", la, None).await.expect("insert la");

        let query = collection
            .query()
            .where_field(
                FieldPath::from_dot_separated("state").unwrap(),
                FilterOperator::Equal,
                FirestoreValue::from_string("California"),
            )
            .unwrap()
            .order_by(FieldPath::from_dot_separated("population").unwrap(), OrderDirection::Descending)
            .unwrap()
            .limit(1)
            .unwrap();

        let snapshot = client.get_docs(&query).await.expect("filtered query");
        assert_eq!(snapshot.len(), 1);
        let doc = &snapshot.documents()[0];
        assert_eq!(doc.id(), "la");
    }
}
