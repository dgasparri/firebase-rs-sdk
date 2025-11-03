use std::sync::Arc;

use async_trait::async_trait;

use crate::firestore::api::query::QueryDefinition;
use crate::firestore::api::DocumentSnapshot;
use crate::firestore::error::FirestoreResult;
use crate::firestore::model::{DocumentKey, FieldPath};
use crate::firestore::value::MapValue;

pub mod http;
pub mod in_memory;

#[derive(Clone, Debug)]
pub enum WriteOperation {
    Set {
        key: DocumentKey,
        data: MapValue,
        mask: Option<Vec<FieldPath>>,
    },
    Update {
        key: DocumentKey,
        data: MapValue,
        field_paths: Vec<FieldPath>,
    },
    Delete {
        key: DocumentKey,
    },
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait Datastore: Send + Sync + 'static {
    async fn get_document(&self, key: &DocumentKey) -> FirestoreResult<DocumentSnapshot>;
    async fn set_document(
        &self,
        key: &DocumentKey,
        data: MapValue,
        mask: Option<Vec<FieldPath>>,
    ) -> FirestoreResult<()>;
    async fn run_query(&self, query: &QueryDefinition) -> FirestoreResult<Vec<DocumentSnapshot>>;
    async fn update_document(
        &self,
        key: &DocumentKey,
        data: MapValue,
        field_paths: Vec<FieldPath>,
    ) -> FirestoreResult<()>;
    async fn delete_document(&self, key: &DocumentKey) -> FirestoreResult<()>;
    async fn commit(&self, writes: Vec<WriteOperation>) -> FirestoreResult<()>;
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait TokenProvider: Send + Sync + 'static {
    async fn get_token(&self) -> FirestoreResult<Option<String>>;
    fn invalidate_token(&self);
    async fn heartbeat_header(&self) -> FirestoreResult<Option<String>> {
        Ok(None)
    }
}

#[derive(Default, Clone)]
pub struct NoopTokenProvider;

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl TokenProvider for NoopTokenProvider {
    async fn get_token(&self) -> FirestoreResult<Option<String>> {
        Ok(None)
    }

    fn invalidate_token(&self) {}
}

pub type TokenProviderArc = Arc<dyn TokenProvider>;

pub use http::{HttpDatastore, RetrySettings};
pub use in_memory::InMemoryDatastore;
