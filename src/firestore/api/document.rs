use std::collections::BTreeMap;

use crate::firestore::api::operations::{self, DocumentSnapshot, SetOptions};
use crate::firestore::error::FirestoreResult;
use crate::firestore::model::DocumentKey;
use std::sync::Arc;

use crate::firestore::remote::datastore::{Datastore, HttpDatastore, InMemoryDatastore, TokenProviderArc};
use crate::firestore::value::{FirestoreValue, MapValue};

use super::Firestore;

#[derive(Clone, Debug)]
pub struct FirestoreClient {
    firestore: Firestore,
    datastore: Arc<dyn Datastore>,
}

impl FirestoreClient {
    pub fn new(firestore: Firestore, datastore: Arc<dyn Datastore>) -> Self {
        Self { firestore, datastore }
    }

    pub fn with_in_memory(firestore: Firestore) -> Self {
        Self::new(firestore, Arc::new(InMemoryDatastore::new()))
    }

    pub fn with_http_datastore(firestore: Firestore) -> FirestoreResult<Self> {
        let datastore = HttpDatastore::from_database_id(firestore.database_id().clone())?;
        Ok(Self::new(firestore, Arc::new(datastore)))
    }

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

    pub fn get_doc(&self, path: &str) -> FirestoreResult<DocumentSnapshot> {
        let key = operations::validate_document_path(path)?;
        self.datastore.get_document(&key)
    }

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::api::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use crate::firestore::api::get_firestore;

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
}
