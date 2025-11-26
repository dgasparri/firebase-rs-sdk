use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use crate::firestore::error::{internal_error, FirestoreResult};
use crate::firestore::model::{DocumentKey, Timestamp};
use crate::firestore::remote::remote_event::{RemoteEvent, TargetChange};
use crate::firestore::remote::watch_change::{
    DocumentChange, DocumentDelete, DocumentRemove, ExistenceFilterChange, TargetChangeState, WatchChange,
    WatchDocument, WatchTargetChange,
};

/// Provides metadata about active targets so the aggregator can reason about
/// existing documents.
pub trait TargetMetadataProvider: Send + Sync {
    fn get_remote_keys(&self, target_id: i32) -> BTreeSet<DocumentKey>;
}

impl TargetMetadataProvider for () {
    fn get_remote_keys(&self, _target_id: i32) -> BTreeSet<DocumentKey> {
        BTreeSet::new()
    }
}

pub struct WatchChangeAggregator<P = ()>
where
    P: TargetMetadataProvider,
{
    metadata: Arc<P>,
    target_states: HashMap<i32, TargetState>,
    target_documents: HashMap<i32, BTreeSet<DocumentKey>>,
    pending_document_updates: BTreeMap<DocumentKey, Option<WatchDocument>>,
    resolved_limbo_documents: BTreeSet<DocumentKey>,
    pending_target_resets: BTreeSet<i32>,
    snapshot_version: Option<Timestamp>,
}

impl<P> WatchChangeAggregator<P>
where
    P: TargetMetadataProvider + 'static,
{
    pub fn new(metadata: Arc<P>) -> Self {
        Self {
            metadata,
            target_states: HashMap::new(),
            target_documents: HashMap::new(),
            pending_document_updates: BTreeMap::new(),
            resolved_limbo_documents: BTreeSet::new(),
            pending_target_resets: BTreeSet::new(),
            snapshot_version: None,
        }
    }

    pub fn handle_watch_change(&mut self, change: WatchChange) -> FirestoreResult<()> {
        match change {
            WatchChange::TargetChange(target_change) => self.handle_target_change(target_change),
            WatchChange::DocumentChange(doc_change) => {
                self.handle_document_change(doc_change);
                Ok(())
            }
            WatchChange::DocumentDelete(delete) => self.handle_document_delete(delete),
            WatchChange::DocumentRemove(remove) => self.handle_document_remove(remove),
            WatchChange::ExistenceFilter(filter) => self.handle_existence_filter(filter),
        }
    }

    fn handle_target_change(&mut self, change: WatchTargetChange) -> FirestoreResult<()> {
        if let Some(error) = change.cause.as_ref() {
            return Err(internal_error(format!(
                "watch target error (code {}): {}",
                error.code.as_str(),
                error
            )));
        }

        let affected: Vec<i32> = if change.target_ids.is_empty() {
            self.target_states.keys().cloned().collect()
        } else {
            change.target_ids.clone()
        };

        for target_id in affected {
            let state = self.target_states.entry(target_id).or_insert_with(TargetState::new);
            self.target_documents
                .entry(target_id)
                .or_insert_with(|| self.metadata.get_remote_keys(target_id));

            match change.state {
                TargetChangeState::NoChange => {
                    state.update_resume_token(change.resume_token.clone());
                }
                TargetChangeState::Add => {
                    state.reset();
                    state.update_resume_token(change.resume_token.clone());
                    state.pending_responses = state.pending_responses.saturating_sub(1);
                }
                TargetChangeState::Remove => {
                    state.pending_responses = state.pending_responses.saturating_sub(1);
                    state.update_resume_token(change.resume_token.clone());
                    self.target_states.remove(&target_id);
                    self.target_documents.remove(&target_id);
                }
                TargetChangeState::Current => {
                    state.current = true;
                    state.update_resume_token(change.resume_token.clone());
                    state.mark_dirty();
                }
                TargetChangeState::Reset => {
                    state.reset();
                    state.update_resume_token(change.resume_token.clone());
                    self.pending_target_resets.insert(target_id);
                }
            }
        }

        Ok(())
    }

    fn handle_document_change(&mut self, change: DocumentChange) {
        let key = change.key.clone();

        if let Some(document) = change.document.clone() {
            for target_id in &change.updated_target_ids {
                self.apply_doc_update(*target_id, key.clone(), Some(document.clone()));
            }
            self.pending_document_updates.insert(key.clone(), Some(document));
        }

        for target_id in &change.removed_target_ids {
            self.apply_doc_update(*target_id, key.clone(), None);
        }

        if change.document.is_none() {
            self.pending_document_updates.insert(key, None);
        }
    }

    fn handle_document_delete(&mut self, delete: DocumentDelete) -> FirestoreResult<()> {
        let key = delete.key.clone();
        for target_id in delete.removed_target_ids {
            self.apply_doc_update(target_id, key.clone(), None);
        }

        self.pending_document_updates.insert(key, None);
        Ok(())
    }

    fn handle_document_remove(&mut self, remove: DocumentRemove) -> FirestoreResult<()> {
        let key = remove.key.clone();
        for target_id in remove.removed_target_ids {
            self.apply_doc_update(target_id, key.clone(), None);
        }
        self.pending_document_updates.insert(key, None);
        Ok(())
    }

    fn handle_existence_filter(&mut self, change: ExistenceFilterChange) -> FirestoreResult<()> {
        self.pending_target_resets.insert(change.target_id);
        Ok(())
    }

    fn apply_doc_update(&mut self, target_id: i32, key: DocumentKey, document: Option<WatchDocument>) {
        let state = self.target_states.entry(target_id).or_insert_with(TargetState::new);
        let docs = self
            .target_documents
            .entry(target_id)
            .or_insert_with(|| self.metadata.get_remote_keys(target_id));

        match document {
            Some(_) => {
                let existed = docs.contains(&key);
                docs.insert(key.clone());
                if existed {
                    state.modified.insert(key.clone());
                } else {
                    state.added.insert(key.clone());
                }
                state.current = false;
                state.mark_dirty();
            }
            None => {
                if docs.remove(&key) {
                    state.removed.insert(key.clone());
                    state.mark_dirty();
                }
            }
        }
    }

    pub fn set_snapshot_version(&mut self, version: Option<Timestamp>) {
        self.snapshot_version = version;
    }

    pub fn drain(&mut self) -> RemoteEvent {
        let target_changes = self
            .target_states
            .iter_mut()
            .filter_map(|(target_id, state)| state.take_changes().map(|change| (*target_id, change)))
            .collect();

        RemoteEvent {
            snapshot_version: self.snapshot_version.take(),
            target_changes,
            target_resets: std::mem::take(&mut self.pending_target_resets),
            document_updates: std::mem::take(&mut self.pending_document_updates),
            resolved_limbo_documents: std::mem::take(&mut self.resolved_limbo_documents),
        }
    }
}

struct TargetState {
    pending_responses: usize,
    resume_token: Option<Vec<u8>>,
    current: bool,
    added: BTreeSet<DocumentKey>,
    modified: BTreeSet<DocumentKey>,
    removed: BTreeSet<DocumentKey>,
    dirty: bool,
}

impl TargetState {
    fn new() -> Self {
        Self {
            pending_responses: 1,
            resume_token: None,
            current: false,
            added: BTreeSet::new(),
            modified: BTreeSet::new(),
            removed: BTreeSet::new(),
            dirty: false,
        }
    }

    fn reset(&mut self) {
        self.added.clear();
        self.modified.clear();
        self.removed.clear();
        self.current = false;
        self.dirty = true;
    }

    fn update_resume_token(&mut self, token: Option<Vec<u8>>) {
        if token.as_ref().map(|t| !t.is_empty()).unwrap_or(false) {
            self.resume_token = token;
            self.dirty = true;
        }
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn take_changes(&mut self) -> Option<TargetChange> {
        if !self.dirty && self.added.is_empty() && self.modified.is_empty() && self.removed.is_empty() {
            return None;
        }

        let change = TargetChange {
            resume_token: self.resume_token.clone(),
            current: self.current,
            added_documents: std::mem::take(&mut self.added),
            modified_documents: std::mem::take(&mut self.modified),
            removed_documents: std::mem::take(&mut self.removed),
        };
        self.dirty = false;
        Some(change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firestore::remote::watch_change::{WatchDocument, WatchTargetChange};
    use crate::firestore::value::MapValue;
    use std::collections::BTreeMap;

    struct TestMetadata;

    impl TargetMetadataProvider for TestMetadata {
        fn get_remote_keys(&self, _target_id: i32) -> BTreeSet<DocumentKey> {
            BTreeSet::new()
        }
    }

    fn doc(path: &str) -> WatchDocument {
        WatchDocument {
            key: DocumentKey::from_string(path).unwrap(),
            fields: MapValue::new(BTreeMap::new()),
            update_time: None,
            create_time: None,
        }
    }

    #[test]
    fn aggregates_document_changes() {
        let metadata = Arc::new(TestMetadata);
        let mut aggregator = WatchChangeAggregator::new(metadata);

        aggregator
            .handle_watch_change(WatchChange::TargetChange(WatchTargetChange {
                state: TargetChangeState::Add,
                target_ids: vec![1],
                resume_token: Some(vec![1, 2, 3]),
                read_time: None,
                cause: None,
            }))
            .unwrap();

        aggregator
            .handle_watch_change(WatchChange::DocumentChange(DocumentChange {
                updated_target_ids: vec![1],
                removed_target_ids: vec![],
                key: DocumentKey::from_string("cities/sf").unwrap(),
                document: Some(doc("cities/sf")),
            }))
            .unwrap();

        let event = aggregator.drain();
        assert_eq!(event.target_changes.len(), 1);
        let change = event.target_changes.get(&1).unwrap();
        assert!(change
            .added_documents
            .contains(&DocumentKey::from_string("cities/sf").unwrap()));
        assert!(event
            .document_updates
            .contains_key(&DocumentKey::from_string("cities/sf").unwrap()));
    }
}
