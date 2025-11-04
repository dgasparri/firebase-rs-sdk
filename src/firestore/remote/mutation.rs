use crate::firestore::model::Timestamp;
use crate::firestore::remote::datastore::WriteOperation;
use crate::firestore::remote::streams::WriteResult;

/// Batch of mutations queued for the streaming write pipeline.
///
/// Mirrors the Firestore JS SDK's `MutationBatch` shape from
/// `packages/firestore/src/model/mutation_batch.ts`, but currently only tracks
/// the information the Rust remote store requires to drive the gRPC write
/// stream.
#[derive(Clone, Debug)]
pub struct MutationBatch {
    /// Monotonic identifier assigned locally when the batch is queued.
    pub batch_id: i32,
    /// Ordered write operations that should be sent to Firestore.
    pub writes: Vec<WriteOperation>,
}

impl MutationBatch {
    /// Creates a new mutation batch with an explicit identifier.
    pub fn new(batch_id: i32, writes: Vec<WriteOperation>) -> Self {
        Self { batch_id, writes }
    }

    /// Returns `true` when the batch contains no writes.
    pub fn is_empty(&self) -> bool {
        self.writes.is_empty()
    }
}

/// Successful acknowledgement of a single mutation batch.
#[derive(Clone, Debug)]
pub struct MutationBatchResult {
    /// Identifier of the acknowledged batch.
    pub batch_id: i32,
    /// Commit timestamp returned by the backend.
    pub commit_version: Option<Timestamp>,
    /// Individual write results produced by the RPC.
    pub write_results: Vec<WriteResult>,
}

impl MutationBatchResult {
    /// Builds a new result payload from the streamed write response.
    pub fn new(
        batch_id: i32,
        commit_version: Option<Timestamp>,
        write_results: Vec<WriteResult>,
    ) -> Self {
        Self {
            batch_id,
            commit_version,
            write_results,
        }
    }
}
