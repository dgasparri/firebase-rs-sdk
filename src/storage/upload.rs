use std::cmp;

use crate::storage::error::{internal_error, invalid_argument, StorageError, StorageResult};
use crate::storage::metadata::ObjectMetadata;
use crate::storage::reference::StorageReference;
use crate::storage::request::{
    continue_resumable_upload_request, create_resumable_upload_request, multipart_upload_request,
    RESUMABLE_UPLOAD_CHUNK_SIZE,
};
use crate::storage::UploadMetadata;

const MAX_RESUMABLE_CHUNK_SIZE: usize = 32 * 1024 * 1024;

/// Represents the execution state of an [`UploadTask`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UploadTaskState {
    Pending,
    Running,
    Completed,
    Error,
    Canceled,
}

/// Progress information emitted while uploading large blobs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UploadProgress {
    pub bytes_transferred: u64,
    pub total_bytes: u64,
}

impl UploadProgress {
    pub fn new(bytes_transferred: u64, total_bytes: u64) -> Self {
        Self {
            bytes_transferred,
            total_bytes,
        }
    }
}

/// Stateful helper that mirrors the Firebase Web SDK's resumable upload behaviour.
///
/// A task is created via [`StorageReference::upload_bytes_resumable`](crate::storage::StorageReference::upload_bytes_resumable)
/// and can then be polled chunk-by-chunk (`upload_next`) or allowed to run to completion (`run_to_completion`).
/// Small payloads are uploaded with a single multipart request, whereas larger blobs utilise the resumable REST API.
pub struct UploadTask {
    reference: StorageReference,
    data: Vec<u8>,
    metadata: Option<UploadMetadata>,
    total_bytes: u64,
    transferred: u64,
    resumable: bool,
    upload_url: Option<String>,
    state: UploadTaskState,
    last_error: Option<StorageError>,
    result_metadata: Option<ObjectMetadata>,
    chunk_multiplier: usize,
}

impl UploadTask {
    pub(crate) fn new(
        reference: StorageReference,
        data: Vec<u8>,
        metadata: Option<UploadMetadata>,
    ) -> Self {
        let total_bytes = data.len() as u64;
        let resumable = total_bytes as usize > RESUMABLE_UPLOAD_CHUNK_SIZE;
        Self {
            reference,
            data,
            metadata,
            total_bytes,
            transferred: 0,
            resumable,
            upload_url: None,
            state: UploadTaskState::Pending,
            last_error: None,
            result_metadata: None,
            chunk_multiplier: 1,
        }
    }

    /// Returns the total number of bytes that will be uploaded.
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// Returns the number of bytes that have been successfully uploaded so far.
    pub fn bytes_transferred(&self) -> u64 {
        self.transferred
    }

    /// Current task state.
    pub fn state(&self) -> UploadTaskState {
        self.state
    }

    /// Last error reported by the task, if any.
    pub fn last_error(&self) -> Option<&StorageError> {
        self.last_error.as_ref()
    }

    /// Resulting object metadata after a successful upload.
    pub fn metadata(&self) -> Option<&ObjectMetadata> {
        self.result_metadata.as_ref()
    }

    /// The resumable session URL, if one has been established.
    pub fn upload_session_url(&self) -> Option<&str> {
        self.upload_url.as_deref()
    }

    /// Uploads the next chunk and invokes the provided progress callback.
    ///
    /// Returns `Ok(Some(metadata))` when the upload finishes and the remote metadata is available.
    pub fn upload_next_with_progress<F>(
        &mut self,
        mut progress: F,
    ) -> StorageResult<Option<ObjectMetadata>>
    where
        F: FnMut(UploadProgress),
    {
        match self.state {
            UploadTaskState::Completed => {
                return Ok(self.result_metadata.clone());
            }
            UploadTaskState::Error => {
                return Err(self
                    .last_error
                    .clone()
                    .unwrap_or_else(|| internal_error("upload task failed")));
            }
            UploadTaskState::Canceled => {
                return Err(invalid_argument("upload task was canceled"));
            }
            _ => {}
        }

        if !self.resumable {
            return self.upload_multipart(progress);
        }

        self.ensure_resumable_session()?;
        self.state = UploadTaskState::Running;

        let storage = self.reference.storage();
        let upload_url = self
            .upload_url
            .clone()
            .ok_or_else(|| internal_error("resumable session url missing"))?;
        let start_offset = self.transferred;
        let chunk_size = self.current_chunk_size() as u64;
        let end_offset = cmp::min(self.total_bytes, start_offset + chunk_size);
        let finalize = end_offset == self.total_bytes;
        let chunk = self
            .data
            .get(start_offset as usize..end_offset as usize)
            .map(|slice| slice.to_vec())
            .unwrap_or_default();

        let request = continue_resumable_upload_request(
            &storage,
            self.reference.location(),
            &upload_url,
            start_offset,
            self.total_bytes,
            chunk,
            finalize,
        );
        let status = match storage.run_upload_request(request) {
            Ok(status) => status,
            Err(err) => {
                self.reset_multiplier();
                return self.fail(err);
            }
        };

        self.transferred = status.current;
        progress(UploadProgress::new(self.transferred, self.total_bytes));

        if status.finalized {
            let metadata = status
                .metadata
                .ok_or_else(|| internal_error("resumable upload completed without metadata"))?;
            self.state = UploadTaskState::Completed;
            self.result_metadata = Some(metadata.clone());
            Ok(Some(metadata))
        } else {
            self.bump_multiplier();
            Ok(None)
        }
    }

    /// Uploads the next chunk without emitting progress callbacks.
    pub fn upload_next(&mut self) -> StorageResult<Option<ObjectMetadata>> {
        self.upload_next_with_progress(|_| {})
    }

    /// Runs the task to completion while notifying `progress` for each chunk.
    pub fn run_to_completion_with_progress<F>(
        mut self,
        mut progress: F,
    ) -> StorageResult<ObjectMetadata>
    where
        F: FnMut(UploadProgress),
    {
        loop {
            match self.upload_next_with_progress(&mut progress)? {
                Some(metadata) => return Ok(metadata),
                None => continue,
            }
        }
    }

    /// Runs the task to completion without progress callbacks.
    pub fn run_to_completion(self) -> StorageResult<ObjectMetadata> {
        self.run_to_completion_with_progress(|_| {})
    }

    fn ensure_resumable_session(&mut self) -> StorageResult<()> {
        if !self.resumable || self.upload_url.is_some() {
            return Ok(());
        }
        let storage = self.reference.storage();
        let request = create_resumable_upload_request(
            &storage,
            self.reference.location(),
            self.metadata.clone(),
            self.total_bytes,
        );
        let url = storage.run_upload_request(request)?;
        self.upload_url = Some(url);
        Ok(())
    }

    fn upload_multipart<F>(&mut self, mut progress: F) -> StorageResult<Option<ObjectMetadata>>
    where
        F: FnMut(UploadProgress),
    {
        if self.state == UploadTaskState::Completed {
            return Ok(self.result_metadata.clone());
        }

        self.state = UploadTaskState::Running;
        let storage = self.reference.storage();
        let request = multipart_upload_request(
            &storage,
            self.reference.location(),
            self.data.clone(),
            self.metadata.clone(),
        );

        match storage.run_upload_request(request) {
            Ok(metadata) => {
                self.transferred = self.total_bytes;
                self.state = UploadTaskState::Completed;
                self.result_metadata = Some(metadata.clone());
                progress(UploadProgress::new(self.transferred, self.total_bytes));
                Ok(Some(metadata))
            }
            Err(err) => self.fail(err),
        }
    }

    fn current_chunk_size(&self) -> usize {
        cmp::min(
            RESUMABLE_UPLOAD_CHUNK_SIZE * self.chunk_multiplier,
            MAX_RESUMABLE_CHUNK_SIZE,
        )
    }

    fn bump_multiplier(&mut self) {
        let next = self.chunk_multiplier * 2;
        if next * RESUMABLE_UPLOAD_CHUNK_SIZE <= MAX_RESUMABLE_CHUNK_SIZE {
            self.chunk_multiplier = next;
        }
    }

    fn reset_multiplier(&mut self) {
        self.chunk_multiplier = 1;
    }

    fn fail<T>(&mut self, error: StorageError) -> StorageResult<T> {
        self.state = UploadTaskState::Error;
        self.last_error = Some(error.clone());
        Err(error)
    }
}
