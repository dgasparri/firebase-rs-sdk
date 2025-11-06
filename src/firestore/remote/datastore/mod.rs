use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
#[cfg(not(target_arch = "wasm32"))]
use futures::future::BoxFuture;
#[cfg(target_arch = "wasm32")]
use futures::future::LocalBoxFuture;

use crate::firestore::AggregateDefinition;
use crate::firestore::FieldTransform;
use crate::firestore::QueryDefinition;
use crate::firestore::api::DocumentSnapshot;
use crate::firestore::error::FirestoreResult;
use crate::firestore::model::{DocumentKey, FieldPath};
use crate::firestore::value::{FirestoreValue, MapValue};

pub mod http;
pub mod in_memory;
pub mod streaming;

#[derive(Clone, Debug)]
pub enum WriteOperation {
    Set {
        key: DocumentKey,
        data: MapValue,
        mask: Option<Vec<FieldPath>>,
        transforms: Vec<FieldTransform>,
    },
    Update {
        key: DocumentKey,
        data: MapValue,
        field_paths: Vec<FieldPath>,
        transforms: Vec<FieldTransform>,
    },
    Delete {
        key: DocumentKey,
    },
}

impl WriteOperation {
    /// Returns the document key targeted by this write.
    pub fn key(&self) -> &DocumentKey {
        match self {
            WriteOperation::Set { key, .. }
            | WriteOperation::Update { key, .. }
            | WriteOperation::Delete { key } => key,
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub type StreamingFuture<'a, T> = LocalBoxFuture<'a, T>;
#[cfg(not(target_arch = "wasm32"))]
pub type StreamingFuture<'a, T> = BoxFuture<'a, T>;

pub trait StreamingDatastore: Send + Sync + 'static {
    fn open_listen_stream(&self) -> StreamingFuture<'_, FirestoreResult<Arc<dyn StreamHandle>>>;
    fn open_write_stream(&self) -> StreamingFuture<'_, FirestoreResult<Arc<dyn StreamHandle>>>;
}

pub trait StreamHandle: Send + Sync + 'static {
    fn send(&self, payload: Vec<u8>) -> StreamingFuture<'_, FirestoreResult<()>>;
    fn next(&self) -> StreamingFuture<'_, Option<FirestoreResult<Vec<u8>>>>;
    fn close(&self) -> StreamingFuture<'_, FirestoreResult<()>>;
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
        transforms: Vec<FieldTransform>,
    ) -> FirestoreResult<()>;
    async fn run_query(&self, query: &QueryDefinition) -> FirestoreResult<Vec<DocumentSnapshot>>;
    async fn update_document(
        &self,
        key: &DocumentKey,
        data: MapValue,
        field_paths: Vec<FieldPath>,
        transforms: Vec<FieldTransform>,
    ) -> FirestoreResult<()>;
    async fn delete_document(&self, key: &DocumentKey) -> FirestoreResult<()>;
    async fn commit(&self, writes: Vec<WriteOperation>) -> FirestoreResult<()>;
    async fn run_aggregate(
        &self,
        query: &QueryDefinition,
        aggregations: &[AggregateDefinition],
    ) -> FirestoreResult<BTreeMap<String, FirestoreValue>>;
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
pub use streaming::StreamingDatastoreImpl;
