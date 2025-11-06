use std::collections::{BTreeMap, BTreeSet};

use crate::firestore::model::{document_key::DocumentKey, timestamp::Timestamp};
use crate::firestore::remote::watch_change::WatchDocument;

/// Aggregated result of applying a batch of watch and write responses coming
/// from Firestore.
#[derive(Debug, Clone, Default)]
pub struct RemoteEvent {
    pub snapshot_version: Option<Timestamp>,
    pub target_changes: BTreeMap<i32, TargetChange>,
    pub target_resets: BTreeSet<i32>,
    pub document_updates: BTreeMap<DocumentKey, Option<WatchDocument>>,
    pub resolved_limbo_documents: BTreeSet<DocumentKey>,
}

impl RemoteEvent {
    pub fn is_empty(&self) -> bool {
        self.target_changes.is_empty()
            && self.document_updates.is_empty()
            && self.target_resets.is_empty()
            && self.resolved_limbo_documents.is_empty()
    }
}

/// Per-target change metadata mirroring the Firestore JS TargetChange type.
#[derive(Debug, Clone, Default)]
pub struct TargetChange {
    pub resume_token: Option<Vec<u8>>,
    pub current: bool,
    pub added_documents: BTreeSet<DocumentKey>,
    pub modified_documents: BTreeSet<DocumentKey>,
    pub removed_documents: BTreeSet<DocumentKey>,
}
