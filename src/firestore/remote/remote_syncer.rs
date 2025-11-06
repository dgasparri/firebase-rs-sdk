use std::collections::BTreeSet;

use futures::FutureExt;

use crate::firestore::error::FirestoreResult;
use crate::firestore::model::document_key::DocumentKey;
use crate::firestore::remote::mutation::{MutationBatch, MutationBatchResult};
use crate::firestore::remote::remote_event::RemoteEvent;
use crate::firestore::remote::streams::write::WriteResult;
use crate::firestore::FirestoreError;

#[cfg(target_arch = "wasm32")]
pub type RemoteStoreFuture<'a, T> = futures::future::LocalBoxFuture<'a, T>;
#[cfg(not(target_arch = "wasm32"))]
pub type RemoteStoreFuture<'a, T> = futures::future::BoxFuture<'a, T>;

#[cfg(target_arch = "wasm32")]
pub fn box_remote_store_future<'a, F, T>(future: F) -> RemoteStoreFuture<'a, T>
where
    F: std::future::Future<Output = T> + 'a,
{
    future.boxed_local()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn box_remote_store_future<'a, F, T>(future: F) -> RemoteStoreFuture<'a, T>
where
    F: std::future::Future<Output = T> + Send + 'a,
{
    future.boxed()
}

/// Bridge between the remote store and the local synchronization engine.
///
/// This mirrors the TypeScript interface defined in
/// `packages/firestore/src/remote/remote_syncer.ts`. Every callback matches the
/// semantics of the JS SDK so that higher layers can be ported incrementally
/// while the remote subsystem stays aligned with the existing behaviour.
pub trait RemoteSyncer: Send + Sync + 'static {
    /// Applies a `RemoteEvent` produced by the watch stream.
    fn apply_remote_event(&self, event: RemoteEvent) -> RemoteStoreFuture<'_, FirestoreResult<()>>;

    /// Signals that a watch target was rejected by the backend.
    fn reject_listen(
        &self,
        target_id: i32,
        error: FirestoreError,
    ) -> RemoteStoreFuture<'_, FirestoreResult<()>>;

    /// Applies the acknowledgement for a committed mutation batch.
    fn apply_successful_write(
        &self,
        result: MutationBatchResult,
    ) -> RemoteStoreFuture<'_, FirestoreResult<()>>;

    /// Rejects a pending mutation batch due to a stream failure.
    fn reject_failed_write(
        &self,
        batch_id: i32,
        error: FirestoreError,
    ) -> RemoteStoreFuture<'_, FirestoreResult<()>>;

    /// Returns the currently cached remote keys for a target.
    fn get_remote_keys_for_target(&self, target_id: i32) -> BTreeSet<DocumentKey>;

    /// Fetches the next pending mutation batch after the provided batch id.
    fn next_mutation_batch(
        &self,
        after_batch_id: Option<i32>,
    ) -> RemoteStoreFuture<'_, FirestoreResult<Option<MutationBatch>>>;

    /// Notifies the syncer that authentication credentials changed.
    fn handle_credential_change(&self) -> RemoteStoreFuture<'_, FirestoreResult<()>> {
        box_remote_store_future(async { Ok(()) })
    }

    /// Updates heartbeat/app check headers for the next write resend.
    fn notify_stream_token_change(&self, _token: Option<Vec<u8>>) {}

    /// Allows the syncer to surface custom write ordering metadata.
    #[allow(unused_variables)]
    fn record_write_results(&self, _batch_id: i32, _results: &[WriteResult]) {}
}
