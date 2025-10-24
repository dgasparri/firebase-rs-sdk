use crate::storage::error::{
    invalid_argument, invalid_root_operation, no_download_url, StorageResult,
};
use crate::storage::list::{parse_list_result, ListOptions, ListResult};
use crate::storage::location::Location;
use crate::storage::metadata::ObjectMetadata;
use crate::storage::path::{child, last_component, parent};
use crate::storage::request::{
    delete_object_request, download_bytes_request, download_url_request, get_metadata_request,
    list_request, multipart_upload_request, update_metadata_request,
};
use crate::storage::service::FirebaseStorageImpl;
use crate::storage::upload::UploadTask;
use crate::storage::{SettableMetadata, UploadMetadata};
use std::convert::TryFrom;

#[derive(Clone)]
pub struct StorageReference {
    storage: FirebaseStorageImpl,
    location: Location,
}

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
    pub async fn update_metadata(
        &self,
        metadata: SettableMetadata,
    ) -> StorageResult<ObjectMetadata> {
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
        let request =
            download_bytes_request(&self.storage, &self.location, max_download_size_bytes);
        let mut bytes = self.storage.run_request(request).await?;

        if let Some(limit) = max_download_size_bytes {
            let limit_usize = usize::try_from(limit).map_err(|_| {
                invalid_argument("max_download_size_bytes exceeds platform addressable memory")
            })?;
            if bytes.len() > limit_usize {
                bytes.truncate(limit_usize);
            }
        }

        Ok(bytes)
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
        let request =
            multipart_upload_request(&self.storage, &self.location, data.into(), metadata);
        self.storage.run_upload_request(request).await
    }

    /// Creates a resumable upload task that can be advanced chunk by chunk or run to completion.
    ///
    /// Resumable uploads stream data in 256 KiB chunks by default, doubling up to 32 MiB to match the
    /// behaviour of the Firebase Web SDK. The returned [`crate::storage::upload::UploadTask`]
    /// exposes helpers to poll chunk progress or upload the entire file with a single call.
    pub fn upload_bytes_resumable(
        &self,
        data: Vec<u8>,
        metadata: Option<UploadMetadata>,
    ) -> StorageResult<UploadTask> {
        self.ensure_not_root("upload_bytes_resumable")?;
        Ok(UploadTask::new(self.clone(), data, metadata))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::api::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};

    fn unique_settings() -> FirebaseAppSettings {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!(
                "storage-ref-{}",
                COUNTER.fetch_add(1, Ordering::SeqCst)
            )),
            ..Default::default()
        }
    }

    fn build_storage() -> FirebaseStorageImpl {
        let options = FirebaseOptions {
            storage_bucket: Some("my-bucket".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let container = app.container();
        let auth_provider = container.get_provider("auth-internal");
        let app_check_provider = container.get_provider("app-check-internal");
        FirebaseStorageImpl::new(app, auth_provider, app_check_provider, None, None).unwrap()
    }

    #[test]
    fn root_reference_has_expected_url() {
        let storage = build_storage();
        let root = storage.root_reference().unwrap();
        assert_eq!(root.to_gs_url(), "gs://my-bucket/");
    }

    #[test]
    fn child_computes_new_path() {
        let storage = build_storage();
        let root = storage.root_reference().unwrap();
        let image = root.child("images/photo.png");
        assert_eq!(image.to_gs_url(), "gs://my-bucket/images/photo.png");
        assert_eq!(image.name(), "photo.png");
        assert_eq!(image.parent().unwrap().to_gs_url(), "gs://my-bucket/images");
    }
}
