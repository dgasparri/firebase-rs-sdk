use std::collections::BTreeMap;

use crate::firestore::api::operations::{self, SetOptions};
use crate::firestore::api::snapshot::{DocumentSnapshot, TypedDocumentSnapshot};
use crate::firestore::error::FirestoreResult;
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
    pub fn get_doc(&self, path: &str) -> FirestoreResult<DocumentSnapshot> {
        let key = operations::validate_document_path(path)?;
        self.datastore.get_document(&key)
    }

    /// Writes the provided map of fields into the document at `path`.
    ///
    /// `options.merge == true` mirrors the JS API but is currently unsupported
    /// for the HTTP datastore.
    pub fn set_doc(
        &self,
        path: &str,
        data: BTreeMap<String, FirestoreValue>,
        options: Option<SetOptions>,
    ) -> FirestoreResult<()> {
        let key = operations::validate_document_path(path)?;
        let encoded = operations::encode_document_data(data)?;
        let merge = options.unwrap_or_default().merge;
        self.datastore.set_document(&key, encoded, merge)
    }

    /// Adds a new document to the collection located at `collection_path` and
    /// returns the resulting snapshot.
    pub fn add_doc(
        &self,
        collection_path: &str,
        data: BTreeMap<String, FirestoreValue>,
    ) -> FirestoreResult<DocumentSnapshot> {
        let collection = self.firestore.collection(collection_path)?;
        let doc_ref = collection.doc(None)?;
        self.set_doc(doc_ref.path().canonical_string().as_str(), data, None)?;
        self.get_doc(doc_ref.path().canonical_string().as_str())
    }

    /// Reads a document using the converter attached to a typed reference.
    pub fn get_doc_with_converter<C>(
        &self,
        reference: &ConvertedDocumentReference<C>,
    ) -> FirestoreResult<TypedDocumentSnapshot<C>>
    where
        C: FirestoreDataConverter,
    {
        let path = reference.path().canonical_string();
        let snapshot = self.get_doc(path.as_str())?;
        let converter = reference.converter();
        Ok(snapshot.into_typed(converter))
    }

    /// Writes a typed model to the location referenced by `reference`.
    pub fn set_doc_with_converter<C>(
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
        self.set_doc(path.as_str(), map, options)
    }

    /// Creates a document with auto-generated ID using the provided converter.
    pub fn add_doc_with_converter<C>(
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
        self.set_doc(path.as_str(), map, None)?;
        let snapshot = self.get_doc(path.as_str())?;
        Ok(snapshot.into_typed(converter))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::api::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use crate::firestore::api::get_firestore;
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

    fn build_client() -> FirestoreClient {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let firestore = get_firestore(Some(app)).unwrap();
        FirestoreClient::with_in_memory(Firestore::from_arc(firestore))
    }

    #[test]
    fn set_and_get_document() {
        let client = build_client();
        let mut data = BTreeMap::new();
        data.insert("name".to_string(), FirestoreValue::from_string("Ada"));
        client
            .set_doc("cities/sf", data.clone(), None)
            .expect("set doc");
        let snapshot = client.get_doc("cities/sf").expect("get doc");
        assert!(snapshot.exists());
        assert_eq!(
            snapshot.data().unwrap().get("name"),
            Some(&FirestoreValue::from_string("Ada"))
        );
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

    #[test]
    fn typed_set_and_get_document() {
        let client = build_client();
        let collection = client.firestore.collection("people").unwrap();
        let converted = collection.with_converter(PersonConverter);
        let doc_ref = converted.doc(Some("ada")).unwrap();

        let person = Person {
            first: "Ada".into(),
            last: "Lovelace".into(),
        };

        client
            .set_doc_with_converter(&doc_ref, person.clone(), None)
            .expect("typed set");

        let snapshot = client.get_doc_with_converter(&doc_ref).expect("typed get");
        assert!(snapshot.exists());
        assert!(snapshot.from_cache());
        assert!(!snapshot.has_pending_writes());

        let decoded = snapshot.data().expect("converter result").unwrap();
        assert_eq!(decoded, person);
    }
}
