use crate::storage::location::Location;
use crate::storage::metadata::ObjectMetadata;
use crate::storage::path::{child, last_component, parent};
use crate::storage::request::get_metadata_request;
use crate::storage::service::FirebaseStorageImpl;

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

    /// Retrieves object metadata from Cloud Storage for this reference.
    pub fn get_metadata(&self) -> crate::storage::error::StorageResult<ObjectMetadata> {
        let request = get_metadata_request(&self.storage, &self.location);
        let json = self.storage.run_request(request)?;
        Ok(ObjectMetadata::from_value(json))
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
