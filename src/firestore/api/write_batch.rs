use std::collections::BTreeMap;
use std::sync::Arc;

use crate::firestore::api::operations::{self, SetOptions};
use crate::firestore::api::{
    converter::FirestoreDataConverter, database::Firestore, reference::ConvertedDocumentReference,
};
use crate::firestore::error::{invalid_argument, resource_exhausted, FirestoreResult};
use crate::firestore::model::DocumentKey;
use crate::firestore::remote::datastore::{Datastore, WriteOperation};
use crate::firestore::value::FirestoreValue;

use super::reference::DocumentReference;

const MAX_BATCH_WRITES: usize = 500;

/// Aggregates write operations and commits them atomically.
///
/// Mirrors the modular JS `writeBatch()` API from
/// `packages/firestore/src/lite-api/write_batch.ts`.
#[derive(Clone)]
pub struct WriteBatch {
    firestore: Firestore,
    datastore: Arc<dyn Datastore>,
    writes: Vec<WriteOperation>,
}

impl WriteBatch {
    pub(crate) fn new(firestore: Firestore, datastore: Arc<dyn Datastore>) -> Self {
        Self {
            firestore,
            datastore,
            writes: Vec::new(),
        }
    }

    /// Adds a set operation to the batch.
    ///
    /// TypeScript reference: `WriteBatch.set` in
    /// `packages/firestore/src/lite-api/write_batch.ts`.
    pub fn set(
        &mut self,
        reference: &DocumentReference,
        data: BTreeMap<String, FirestoreValue>,
        options: Option<SetOptions>,
    ) -> FirestoreResult<&mut Self> {
        self.ensure_capacity()?;
        self.ensure_same_firestore(reference.firestore())?;
        let key = DocumentKey::from_path(reference.path().clone())?;
        let options = options.unwrap_or_default();
        let encoded = operations::encode_set_data(data, &options)?;
        self.writes.push(WriteOperation::Set {
            key,
            data: encoded.map,
            mask: encoded.mask,
            transforms: encoded.transforms,
        });
        Ok(self)
    }

    /// Adds a typed set operation using the provided converter.
    ///
    /// TypeScript reference: `WriteBatch.set` (converter overload) in
    /// `packages/firestore/src/lite-api/write_batch.ts`.
    pub fn set_with_converter<C>(
        &mut self,
        reference: &ConvertedDocumentReference<C>,
        model: C::Model,
        options: Option<SetOptions>,
    ) -> FirestoreResult<&mut Self>
    where
        C: FirestoreDataConverter,
    {
        let converter = reference.converter();
        let map = converter.to_map(&model)?;
        self.set(reference.raw(), map, options)
    }

    /// Adds an update operation to the batch.
    ///
    /// TypeScript reference: `WriteBatch.update` in
    /// `packages/firestore/src/lite-api/write_batch.ts`.
    pub fn update(
        &mut self,
        reference: &DocumentReference,
        data: BTreeMap<String, FirestoreValue>,
    ) -> FirestoreResult<&mut Self> {
        self.ensure_capacity()?;
        self.ensure_same_firestore(reference.firestore())?;
        let key = DocumentKey::from_path(reference.path().clone())?;
        let encoded = operations::encode_update_document_data(data)?;
        self.writes.push(WriteOperation::Update {
            key,
            data: encoded.map,
            field_paths: encoded.field_paths,
            transforms: encoded.transforms,
        });
        Ok(self)
    }

    /// Adds an update operation for a converted reference.
    pub fn update_with_converter<C>(
        &mut self,
        reference: &ConvertedDocumentReference<C>,
        data: BTreeMap<String, FirestoreValue>,
    ) -> FirestoreResult<&mut Self>
    where
        C: FirestoreDataConverter,
    {
        self.update(reference.raw(), data)
    }

    /// Adds a delete operation to the batch.
    ///
    /// TypeScript reference: `WriteBatch.delete` in
    /// `packages/firestore/src/lite-api/write_batch.ts`.
    pub fn delete(&mut self, reference: &DocumentReference) -> FirestoreResult<&mut Self> {
        self.ensure_capacity()?;
        self.ensure_same_firestore(reference.firestore())?;
        let key = DocumentKey::from_path(reference.path().clone())?;
        self.writes.push(WriteOperation::Delete { key });
        Ok(self)
    }

    /// Adds a delete operation for a converted reference.
    pub fn delete_with_converter<C>(
        &mut self,
        reference: &ConvertedDocumentReference<C>,
    ) -> FirestoreResult<&mut Self>
    where
        C: FirestoreDataConverter,
    {
        self.delete(reference.raw())
    }

    /// Commits all queued writes atomically.
    ///
    /// TypeScript reference: `WriteBatch.commit` in
    /// `packages/firestore/src/lite-api/write_batch.ts`.
    pub async fn commit(self) -> FirestoreResult<()> {
        self.datastore.commit(self.writes).await
    }

    fn ensure_same_firestore(&self, other: &Firestore) -> FirestoreResult<()> {
        if self.firestore.database_id() != other.database_id() {
            return Err(invalid_argument(
                "All WriteBatch operations must target the same Firestore instance",
            ));
        }
        Ok(())
    }

    fn ensure_capacity(&self) -> FirestoreResult<()> {
        if self.writes.len() >= MAX_BATCH_WRITES {
            return Err(resource_exhausted(
                "WriteBatch cannot contain more than 500 operations",
            ));
        }
        Ok(())
    }
}
