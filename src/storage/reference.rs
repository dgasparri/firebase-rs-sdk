use crate::storage::error::{internal_error, invalid_argument, invalid_root_operation, no_download_url, StorageResult};
use crate::storage::list::{parse_list_result, ListOptions, ListResult};
use crate::storage::location::Location;
use crate::storage::metadata::serde::ObjectMetadata;
use crate::storage::path::{child, last_component, parent};
#[cfg(not(target_arch = "wasm32"))]
use crate::storage::request::StreamingResponse;
use crate::storage::request::{
    continue_resumable_upload_request, create_resumable_upload_request, delete_object_request, download_bytes_request,
    download_url_request, get_metadata_request, list_request, multipart_upload_request, update_metadata_request,
    RESUMABLE_UPLOAD_CHUNK_SIZE,
};
use crate::storage::service::FirebaseStorageImpl;
use crate::storage::stream::UploadAsyncRead;
use crate::storage::string::{prepare_string_upload, StringFormat};
use crate::storage::upload::{UploadProgress, UploadTask};
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use crate::storage::wasm;
use crate::storage::{SettableMetadata, UploadMetadata};
use std::convert::TryFrom;

#[derive(Clone)]
pub struct StorageReference {
    storage: FirebaseStorageImpl,
    location: Location,
}

#[cfg(not(target_arch = "wasm32"))]
pub type StreamingDownload = StreamingResponse;

impl StorageReference {
    pub(crate) fn new(storage: FirebaseStorageImpl, location: Location) -> Self {
        Self { storage, location }
    }

    pub fn storage(&self) -> FirebaseStorageImpl {
        self.storage.clone()
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn to_gs_url(&self) -> String {
        if self.location.path().is_empty() {
            format!("gs://{}/", self.location.bucket())
        } else {
            format!("gs://{}/{}", self.location.bucket(), self.location.path())
        }
    }

    pub fn root(&self) -> StorageReference {
        let location = Location::new(self.location.bucket(), "");
        StorageReference::new(self.storage.clone(), location)
    }

    pub fn bucket(&self) -> &str {
        self.location.bucket()
    }

    pub fn full_path(&self) -> &str {
        self.location.path()
    }

    pub fn name(&self) -> String {
        last_component(self.location.path())
    }

    pub fn parent(&self) -> Option<StorageReference> {
        let path = parent(self.location.path())?;
        let location = Location::new(self.location.bucket(), path);
        Some(StorageReference::new(self.storage.clone(), location))
    }

    pub fn child(&self, segment: &str) -> StorageReference {
        let new_path = child(self.location.path(), segment);
        let location = Location::new(self.location.bucket(), new_path);
        StorageReference::new(self.storage.clone(), location)
    }

    fn ensure_not_root(&self, operation: &str) -> StorageResult<()> {
        if self.location.is_root() {
            Err(invalid_root_operation(operation))
        } else {
            Ok(())
        }
    }

    /// Retrieves object metadata from Cloud Storage for this reference.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`](crate::storage::StorageError) with code
    /// `storage/invalid-root-operation` if the reference points to the bucket root.
    pub async fn get_metadata(&self) -> StorageResult<ObjectMetadata> {
        self.ensure_not_root("get_metadata")?;
        let request = get_metadata_request(&self.storage, &self.location);
        let json = self.storage.run_request(request).await?;
        Ok(ObjectMetadata::from_value(json))
    }

    /// Lists objects and prefixes immediately under this reference.
    pub async fn list(&self, options: Option<ListOptions>) -> StorageResult<ListResult> {
        let opts = options.unwrap_or_default();
        let request = list_request(&self.storage, &self.location, &opts);
        let json = self.storage.run_request(request).await?;
        parse_list_result(&self.storage, self.location.bucket(), json)
    }

    /// Recursively lists all objects beneath this reference.
    ///
    /// This mirrors the Firebase Web SDK `listAll` helper and repeatedly calls [`list`](Self::list)
    /// until the backend stops returning a `nextPageToken`.
    pub async fn list_all(&self) -> StorageResult<ListResult> {
        let mut merged = ListResult::default();
        let mut page_token: Option<String> = None;

        loop {
            let mut options = ListOptions::default();
            options.page_token = page_token.clone();
            let page = self.list(Some(options)).await?;
            merged.prefixes.extend(page.prefixes);
            merged.items.extend(page.items);

            if let Some(token) = page.next_page_token {
                page_token = Some(token);
            } else {
                break;
            }
        }

        Ok(merged)
    }

    /// Updates mutable metadata fields for this object.
    ///
    /// # Errors
    ///
    /// Returns [`storage/invalid-root-operation`](crate::storage::StorageErrorCode::InvalidRootOperation)
    /// when invoked on the bucket root.
    pub async fn update_metadata(&self, metadata: SettableMetadata) -> StorageResult<ObjectMetadata> {
        self.ensure_not_root("update_metadata")?;
        let request = update_metadata_request(&self.storage, &self.location, metadata);
        let json = self.storage.run_request(request).await?;
        Ok(ObjectMetadata::from_value(json))
    }

    /// Downloads the referenced object into memory as a byte vector.
    ///
    /// The optional `max_download_size_bytes` mirrors the Web SDK behaviour: when supplied the
    /// backend is asked for at most that many bytes and the response is truncated if the server
    /// ignores the range header.
    pub async fn get_bytes(&self, max_download_size_bytes: Option<u64>) -> StorageResult<Vec<u8>> {
        self.ensure_not_root("get_bytes")?;
        let request = download_bytes_request(&self.storage, &self.location, max_download_size_bytes);
        let mut bytes = self.storage.run_request(request).await?;

        if let Some(limit) = max_download_size_bytes {
            let limit_usize = usize::try_from(limit)
                .map_err(|_| invalid_argument("max_download_size_bytes exceeds platform addressable memory"))?;
            if bytes.len() > limit_usize {
                bytes.truncate(limit_usize);
            }
        }

        Ok(bytes)
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Streams the referenced object as an async reader without buffering the entire payload.
    ///
    /// Returns a [`StreamingResponse`] whose [`StorageByteStream`] can be consumed using the
    /// standard `tokio::io::AsyncRead` interfaces.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// # use std::error::Error;
    /// # use tokio::io::{AsyncReadExt, copy};
    /// # use tokio::fs::File;
    /// # use firebase_rs_sdk::storage::StorageReference;
    /// # async fn example(reference: StorageReference) -> Result<(), Box<dyn Error>> {
    /// let response = reference.get_stream(None).await?;
    /// let mut reader = response.reader;
    /// let mut file = File::create("download.bin").await?;
    /// copy(&mut reader, &mut file).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_stream(&self, max_download_size_bytes: Option<u64>) -> StorageResult<StreamingResponse> {
        self.ensure_not_root("get_stream")?;
        let request = download_bytes_request(&self.storage, &self.location, max_download_size_bytes);
        self.storage.run_streaming_request(request).await
    }

    /// Returns a signed download URL for the object.
    pub async fn get_download_url(&self) -> StorageResult<String> {
        self.ensure_not_root("get_download_url")?;
        let request = download_url_request(&self.storage, &self.location);
        let url = self.storage.run_request(request).await?;
        url.ok_or_else(no_download_url)
    }

    /// Permanently deletes the object referenced by this path.
    pub async fn delete_object(&self) -> StorageResult<()> {
        self.ensure_not_root("delete_object")?;
        let request = delete_object_request(&self.storage, &self.location);
        self.storage.run_request(request).await
    }

    /// Uploads a small blob in a single multipart request.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::error::Error;
    /// # use firebase_rs_sdk::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};
    /// # use firebase_rs_sdk::storage::get_storage_for_app;
    ///
    /// # async fn run() -> Result<(), Box<dyn Error>> {
    /// let options = FirebaseOptions {
    ///     storage_bucket: Some("my-bucket".into()),
    ///     ..Default::default()
    /// };
    /// let app = initialize_app(options, Some(FirebaseAppSettings::default())).await?;
    /// let storage = get_storage_for_app(Some(app), None).await?;
    /// let avatar = storage.root_reference().unwrap().child("avatars/user.png");
    /// avatar.upload_bytes(vec![0_u8; 1024], None).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn upload_bytes(
        &self,
        data: impl Into<Vec<u8>>,
        metadata: Option<UploadMetadata>,
    ) -> StorageResult<ObjectMetadata> {
        self.ensure_not_root("upload_bytes")?;
        let request = multipart_upload_request(&self.storage, &self.location, data.into(), metadata);
        self.storage.run_upload_request(request).await
    }

    /// Creates a resumable upload task that can be advanced chunk by chunk or run to completion.
    ///
    /// Resumable uploads stream data in 256 KiB chunks by default, doubling up to 32 MiB to match the
    /// behaviour of the Firebase Web SDK. The returned [`crate::storage::upload::UploadTask`]
    /// exposes helpers to poll chunk progress or upload the entire file with a single call.
    pub fn upload_bytes_resumable(&self, data: Vec<u8>, metadata: Option<UploadMetadata>) -> StorageResult<UploadTask> {
        self.ensure_not_root("upload_bytes_resumable")?;
        Ok(UploadTask::new(self.clone(), data, metadata))
    }

    /// Uploads a string using the specified [`StringFormat`], mirroring the Web SDK's `uploadString` helper.
    pub async fn upload_string(
        &self,
        data: &str,
        format: StringFormat,
        metadata: Option<UploadMetadata>,
    ) -> StorageResult<ObjectMetadata> {
        self.ensure_not_root("upload_string")?;
        let prepared = prepare_string_upload(data, format)?;
        let metadata = merge_metadata(metadata, prepared.content_type);
        self.upload_bytes(prepared.bytes, metadata).await
    }

    #[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
    /// Uploads a [`web_sys::Blob`] by reading it into memory and delegating to [`upload_bytes`].
    pub async fn upload_blob(
        &self,
        blob: &web_sys::Blob,
        metadata: Option<UploadMetadata>,
    ) -> StorageResult<ObjectMetadata> {
        let data = wasm::blob_to_vec(blob).await?;
        self.upload_bytes(data, metadata).await
    }

    #[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
    /// Creates a resumable upload task backed by the contents of a [`web_sys::Blob`].
    pub async fn upload_blob_resumable(
        &self,
        blob: &web_sys::Blob,
        metadata: Option<UploadMetadata>,
    ) -> StorageResult<UploadTask> {
        let data = wasm::blob_to_vec(blob).await?;
        self.upload_bytes_resumable(data, metadata)
    }

    #[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
    /// Uploads the contents of a [`js_sys::Uint8Array`].
    pub async fn upload_uint8_array(
        &self,
        data: &js_sys::Uint8Array,
        metadata: Option<UploadMetadata>,
    ) -> StorageResult<ObjectMetadata> {
        self.upload_bytes(wasm::uint8_array_to_vec(data), metadata).await
    }

    #[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
    /// Creates a resumable upload task from a [`js_sys::Uint8Array`].
    pub fn upload_uint8_array_resumable(
        &self,
        data: &js_sys::Uint8Array,
        metadata: Option<UploadMetadata>,
    ) -> StorageResult<UploadTask> {
        self.upload_bytes_resumable(wasm::uint8_array_to_vec(data), metadata)
    }

    #[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
    /// Downloads the object as a [`web_sys::Blob`], matching the Web SDK's `getBlob`.
    pub async fn get_blob(&self, max_download_size_bytes: Option<u64>) -> StorageResult<web_sys::Blob> {
        let bytes = self.get_bytes(max_download_size_bytes).await?;
        wasm::bytes_to_blob(&bytes)
    }

    #[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
    /// Streams data from a [`web_sys::ReadableStream`] via the resumable upload pipeline.
    pub async fn upload_readable_stream_resumable(
        &self,
        stream: &web_sys::ReadableStream,
        total_size: u64,
        metadata: Option<UploadMetadata>,
    ) -> StorageResult<ObjectMetadata> {
        let reader = wasm::readable_stream_async_reader(stream)?;
        self.upload_reader_resumable(reader, total_size, metadata).await
    }

    /// Streams data from an [`AsyncRead`](futures::io::AsyncRead) source using the resumable upload API.
    pub async fn upload_reader_resumable<R>(
        &self,
        reader: R,
        total_size: u64,
        metadata: Option<UploadMetadata>,
    ) -> StorageResult<ObjectMetadata>
    where
        R: UploadAsyncRead,
    {
        self.upload_reader_resumable_with_progress(reader, total_size, metadata, |_| {})
            .await
    }

    /// Streams data from an [`AsyncRead`](futures::io::AsyncRead) source while reporting chunk progress.
    pub async fn upload_reader_resumable_with_progress<R, F>(
        &self,
        mut reader: R,
        total_size: u64,
        metadata: Option<UploadMetadata>,
        mut progress: F,
    ) -> StorageResult<ObjectMetadata>
    where
        R: UploadAsyncRead,
        F: FnMut(UploadProgress),
    {
        use futures::io::AsyncReadExt;

        self.ensure_not_root("upload_reader_resumable")?;

        let storage = self.storage();
        let request = create_resumable_upload_request(&storage, self.location(), metadata, total_size);
        let upload_url = storage.run_upload_request(request).await?;

        if total_size == 0 {
            let request =
                continue_resumable_upload_request(&storage, self.location(), &upload_url, 0, 0, Vec::new(), true);
            let status = storage.run_upload_request(request).await?;
            progress(UploadProgress::new(0, 0));
            let metadata = status
                .metadata
                .ok_or_else(|| internal_error("resumable upload completed without metadata"))?;
            return Ok(metadata);
        }

        let chunk_size = RESUMABLE_UPLOAD_CHUNK_SIZE as usize;
        let mut buffer = vec![0u8; chunk_size];
        let mut offset = 0u64;

        while offset < total_size {
            let remaining = (total_size - offset) as usize;
            let to_read = remaining.min(chunk_size);
            let mut read_total = 0usize;

            while read_total < to_read {
                let read = reader
                    .read(&mut buffer[read_total..to_read])
                    .await
                    .map_err(|err| internal_error(format!("failed to read from upload source: {err}")))?;
                if read == 0 {
                    break;
                }
                read_total += read;
            }

            if read_total == 0 {
                return Err(internal_error("upload source ended before the declared total_size was reached"));
            }

            let finalize = offset + read_total as u64 == total_size;
            let chunk = buffer[..read_total].to_vec();

            let request = continue_resumable_upload_request(
                &storage,
                self.location(),
                &upload_url,
                offset,
                total_size,
                chunk,
                finalize,
            );
            let status = storage.run_upload_request(request).await?;
            offset = status.current;
            progress(UploadProgress::new(offset, total_size));

            if finalize {
                let metadata = status
                    .metadata
                    .ok_or_else(|| internal_error("resumable upload completed without metadata"))?;
                return Ok(metadata);
            }
        }

        let request = continue_resumable_upload_request(
            &storage,
            self.location(),
            &upload_url,
            offset,
            total_size,
            Vec::new(),
            true,
        );
        let status = storage.run_upload_request(request).await?;
        progress(UploadProgress::new(offset, total_size));
        let metadata = status
            .metadata
            .ok_or_else(|| internal_error("resumable upload completed without metadata"))?;
        Ok(metadata)
    }
}

fn merge_metadata(metadata: Option<UploadMetadata>, inferred_content_type: Option<String>) -> Option<UploadMetadata> {
    match (metadata, inferred_content_type) {
        (Some(mut metadata), Some(content_type)) => {
            if metadata.content_type.is_none() {
                metadata.content_type = Some(content_type);
            }
            Some(metadata)
        }
        (Some(metadata), None) => Some(metadata),
        (None, Some(content_type)) => {
            let mut metadata = UploadMetadata::new();
            metadata.content_type = Some(content_type);
            Some(metadata)
        }
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};

    fn unique_settings() -> FirebaseAppSettings {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!("storage-ref-{}", COUNTER.fetch_add(1, Ordering::SeqCst))),
            ..Default::default()
        }
    }

    async fn build_storage() -> FirebaseStorageImpl {
        let options = FirebaseOptions {
            storage_bucket: Some("my-bucket".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).await.unwrap();
        let container = app.container();
        let auth_provider = container.get_provider("auth-internal");
        let app_check_provider = container.get_provider("app-check-internal");
        FirebaseStorageImpl::new(app, auth_provider, app_check_provider, None, None).unwrap()
    }

    #[tokio::test]
    async fn root_reference_has_expected_url() {
        let storage = build_storage().await;
        let root = storage.root_reference().unwrap();
        assert_eq!(root.to_gs_url(), "gs://my-bucket/");
    }

    #[tokio::test]
    async fn child_computes_new_path() {
        let storage = build_storage().await;
        let root = storage.root_reference().unwrap();
        let image = root.child("images/photo.png");
        assert_eq!(image.to_gs_url(), "gs://my-bucket/images/photo.png");
        assert_eq!(image.name(), "photo.png");
        assert_eq!(image.parent().unwrap().to_gs_url(), "gs://my-bucket/images");
    }

    #[test]
    fn merge_metadata_preserves_existing_content_type() {
        let original = UploadMetadata::new().with_content_type("image/png");
        let merged = merge_metadata(Some(original.clone()), Some("text/plain".to_string())).unwrap();
        assert_eq!(merged.content_type.as_deref(), Some("image/png"));
    }

    #[test]
    fn merge_metadata_uses_inferred_when_absent() {
        let merged = merge_metadata(None, Some("text/plain".to_string())).unwrap();
        assert_eq!(merged.content_type.as_deref(), Some("text/plain"));
    }
}
