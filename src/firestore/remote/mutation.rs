use std::collections::BTreeMap;

use crate::firestore::model::{document_key::DocumentKey, timestamp::Timestamp};
use crate::firestore::remote::datastore::WriteOperation;
use crate::firestore::remote::streams::write::WriteResult;
use crate::firestore::FirestoreResult;

/// Batch of mutations queued for the streaming write pipeline.
///
/// Mirrors the Firestore JS SDK's `MutationBatch` shape from
/// `packages/firestore/src/model/mutation_batch.ts`, including local write
/// timestamps and base writes used for latency compensation.
#[derive(Clone, Debug)]
pub struct MutationBatch {
    /// Monotonic identifier assigned locally when the batch is queued.
    pub batch_id: i32,
    /// Client-side time at which the batch was created.
    pub local_write_time: Timestamp,
    /// Writes that should only affect the local view (never sent to the backend).
    pub base_writes: Vec<WriteOperation>,
    /// Ordered write operations that should be sent to Firestore.
    pub writes: Vec<WriteOperation>,
}

impl MutationBatch {
    /// Creates a new mutation batch with explicit metadata.
    ///
    /// TypeScript reference: `MutationBatch` constructor in
    /// `packages/firestore/src/model/mutation_batch.ts`.
    pub fn new(
        batch_id: i32,
        local_write_time: Timestamp,
        base_writes: Vec<WriteOperation>,
        writes: Vec<WriteOperation>,
    ) -> Self {
        Self {
            batch_id,
            local_write_time,
            base_writes,
            writes,
        }
    }

    /// Convenience constructor for batches without base writes.
    pub fn from_writes(
        batch_id: i32,
        local_write_time: Timestamp,
        writes: Vec<WriteOperation>,
    ) -> Self {
        Self::new(batch_id, local_write_time, Vec::new(), writes)
    }

    /// Returns `true` when the batch contains no user writes.
    pub fn is_empty(&self) -> bool {
        self.writes.is_empty()
    }

    /// Collects the document keys affected by the user-facing writes.
    pub fn document_keys(&self) -> Vec<DocumentKey> {
        self.writes
            .iter()
            .map(|write| write.key().clone())
            .collect()
    }
}

/// Successful acknowledgement of a single mutation batch.
#[derive(Clone, Debug)]
pub struct MutationBatchResult {
    /// Mutation batch that was acknowledged by the backend.
    pub batch: MutationBatch,
    /// Commit timestamp returned by the backend.
    pub commit_version: Option<Timestamp>,
    /// Individual write results produced by the RPC.
    pub write_results: Vec<WriteResult>,
    /// Mapping from each mutated document to the resulting remote version.
    pub doc_versions: BTreeMap<DocumentKey, Option<Timestamp>>,
}

impl MutationBatchResult {
    /// Builds a new result payload from the streamed write response.
    ///
    /// TypeScript reference: `MutationBatchResult.from` in
    /// `packages/firestore/src/model/mutation_batch.ts`.
    pub fn from(
        batch: MutationBatch,
        commit_version: Option<Timestamp>,
        write_results: Vec<WriteResult>,
    ) -> FirestoreResult<Self> {
        if batch.writes.len() != write_results.len() {
            return Err(crate::firestore::error::internal_error(format!(
                "Mutation batch {} expected {} write results but received {}",
                batch.batch_id,
                batch.writes.len(),
                write_results.len()
            )));
        }

        let mut doc_versions = BTreeMap::new();
        for (write, result) in batch.writes.iter().zip(write_results.iter()) {
            let version = result.update_time.or(commit_version);
            doc_versions.insert(write.key().clone(), version);
        }

        Ok(Self {
            batch,
            commit_version,
            write_results,
            doc_versions,
        })
    }

    /// Identifier of the acknowledged batch.
    pub fn batch_id(&self) -> i32 {
        self.batch.batch_id
    }
}
