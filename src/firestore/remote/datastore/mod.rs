use std::sync::Arc;

use async_trait::async_trait;

use crate::firestore::api::query::QueryDefinition;
use crate::firestore::api::DocumentSnapshot;
use crate::firestore::error::FirestoreResult;
use crate::firestore::model::DocumentKey;
use crate::firestore::value::MapValue;

pub mod http;
pub mod in_memory;

#[async_trait]
pub trait Datastore: Send + Sync + 'static {
    async fn get_document(&self, key: &DocumentKey) -> FirestoreResult<DocumentSnapshot>;
    async fn set_document(
        &self,
        key: &DocumentKey,
        data: MapValue,
        merge: bool,
    ) -> FirestoreResult<()>;
    async fn run_query(&self, query: &QueryDefinition) -> FirestoreResult<Vec<DocumentSnapshot>>;
}

#[async_trait]
pub trait TokenProvider: Send + Sync + 'static {
    async fn get_token(&self) -> FirestoreResult<Option<String>>;
    fn invalidate_token(&self);
}

#[derive(Default, Clone)]
pub struct NoopTokenProvider;

#[async_trait]
impl TokenProvider for NoopTokenProvider {
    async fn get_token(&self) -> FirestoreResult<Option<String>> {
        Ok(None)
    }

    fn invalidate_token(&self) {}
}

pub type TokenProviderArc = Arc<dyn TokenProvider>;

pub use http::{HttpDatastore, RetrySettings};
pub use in_memory::InMemoryDatastore;
