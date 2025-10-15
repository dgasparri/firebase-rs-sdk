use std::sync::Arc;

use crate::firestore::api::query::QueryDefinition;
use crate::firestore::api::DocumentSnapshot;
use crate::firestore::error::FirestoreResult;
use crate::firestore::model::DocumentKey;
use crate::firestore::value::MapValue;

pub mod http;
pub mod in_memory;

pub trait Datastore: Send + Sync + 'static {
    fn get_document(&self, key: &DocumentKey) -> FirestoreResult<DocumentSnapshot>;
    fn set_document(&self, key: &DocumentKey, data: MapValue, merge: bool) -> FirestoreResult<()>;
    fn run_query(&self, query: &QueryDefinition) -> FirestoreResult<Vec<DocumentSnapshot>>;
}

pub trait TokenProvider: Send + Sync + 'static {
    fn get_token(&self) -> FirestoreResult<Option<String>>;
    fn invalidate_token(&self);
}

#[derive(Default, Clone)]
pub struct NoopTokenProvider;

impl TokenProvider for NoopTokenProvider {
    fn get_token(&self) -> FirestoreResult<Option<String>> {
        Ok(None)
    }

    fn invalidate_token(&self) {}
}

pub type TokenProviderArc = Arc<dyn TokenProvider>;

pub use http::{HttpDatastore, RetrySettings};
pub use in_memory::InMemoryDatastore;
