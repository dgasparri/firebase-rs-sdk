use std::sync::{Arc, Mutex};

use crate::app::FirebaseApp;
use crate::component::Provider;
use crate::storage::constants::{
    DEFAULT_HOST, DEFAULT_MAX_OPERATION_RETRY_TIME_MS, DEFAULT_MAX_UPLOAD_RETRY_TIME_MS,
    DEFAULT_PROTOCOL,
};
use crate::storage::error::{no_default_bucket, StorageResult};
use crate::storage::location::Location;
use crate::storage::reference::StorageReference;
use crate::storage::util::is_url;

#[derive(Clone)]
pub struct FirebaseStorageImpl {
    app: FirebaseApp,
    auth_provider: Provider,
    app_check_provider: Provider,
    firebase_version: Option<String>,
    url_override: Option<String>,
    state: Arc<Mutex<FirebaseStorageState>>,
}

struct FirebaseStorageState {
    bucket: Option<Location>,
    host: String,
    protocol: String,
    max_operation_retry_time_ms: u64,
    max_upload_retry_time_ms: u64,
    override_auth_token: Option<String>,
    is_using_emulator: bool,
}

impl FirebaseStorageImpl {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        app: FirebaseApp,
        auth_provider: Provider,
        app_check_provider: Provider,
        url_override: Option<String>,
        firebase_version: Option<String>,
    ) -> StorageResult<Self> {
        let host = DEFAULT_HOST.to_string();
        let bucket = if let Some(url) = url_override.as_ref() {
            Some(Location::from_bucket_spec(url, &host)?)
        } else {
            extract_bucket(&host, &app)?
        };

        let state = FirebaseStorageState {
            bucket,
            host,
            protocol: DEFAULT_PROTOCOL.to_string(),
            max_operation_retry_time_ms: DEFAULT_MAX_OPERATION_RETRY_TIME_MS,
            max_upload_retry_time_ms: DEFAULT_MAX_UPLOAD_RETRY_TIME_MS,
            override_auth_token: None,
            is_using_emulator: false,
        };

        Ok(Self {
            app,
            auth_provider,
            app_check_provider,
            firebase_version,
            url_override,
            state: Arc::new(Mutex::new(state)),
        })
    }

    pub fn app(&self) -> &FirebaseApp {
        &self.app
    }

    pub fn host(&self) -> String {
        self.state.lock().unwrap().host.clone()
    }

    pub fn protocol(&self) -> String {
        self.state.lock().unwrap().protocol.clone()
    }

    pub fn auth_provider(&self) -> Provider {
        self.auth_provider.clone()
    }

    pub fn app_check_provider(&self) -> Provider {
        self.app_check_provider.clone()
    }

    pub fn firebase_version(&self) -> Option<&str> {
        self.firebase_version.as_deref()
    }

    pub fn bucket(&self) -> Option<Location> {
        self.state.lock().unwrap().bucket.clone()
    }

    pub fn max_upload_retry_time(&self) -> u64 {
        self.state.lock().unwrap().max_upload_retry_time_ms
    }

    pub fn max_operation_retry_time(&self) -> u64 {
        self.state.lock().unwrap().max_operation_retry_time_ms
    }

    pub fn set_max_upload_retry_time(&self, millis: u64) {
        self.state.lock().unwrap().max_upload_retry_time_ms = millis;
    }

    pub fn set_max_operation_retry_time(&self, millis: u64) {
        self.state.lock().unwrap().max_operation_retry_time_ms = millis;
    }

    pub fn is_using_emulator(&self) -> bool {
        self.state.lock().unwrap().is_using_emulator
    }

    pub fn connect_emulator(
        &self,
        host: &str,
        port: u16,
        mock_user_token: Option<String>,
    ) -> StorageResult<()> {
        let host_string = format!("{host}:{port}");
        let bucket = self.compute_bucket_for_host(&host_string)?;
        let mut state = self.state.lock().unwrap();
        state.host = host_string;
        state.bucket = bucket;
        state.protocol = "http".to_string();
        state.is_using_emulator = true;
        state.override_auth_token = mock_user_token;
        Ok(())
    }

    pub fn set_host(&self, host: &str) -> StorageResult<()> {
        let bucket = self.compute_bucket_for_host(host)?;
        let mut state = self.state.lock().unwrap();
        state.host = host.to_string();
        state.bucket = bucket;
        Ok(())
    }

    fn compute_bucket_for_host(&self, host: &str) -> StorageResult<Option<Location>> {
        if let Some(url) = self.url_override.as_ref() {
            Ok(Some(Location::from_bucket_spec(url, host)?))
        } else {
            extract_bucket(host, &self.app)
        }
    }

    pub fn make_storage_reference(&self, location: Location) -> StorageReference {
        StorageReference::new(self.clone(), location)
    }

    pub fn root_reference(&self) -> StorageResult<StorageReference> {
        let state = self.state.lock().unwrap();
        let bucket = state.bucket.clone().ok_or_else(no_default_bucket)?;
        Ok(StorageReference::new(self.clone(), bucket))
    }

    pub fn reference_from_path(&self, path: Option<&str>) -> StorageResult<StorageReference> {
        let location = match path {
            Some(path) if is_url(path) => Location::from_url(path, &self.host())?,
            Some(path) => {
                let base = self.bucket().ok_or_else(no_default_bucket)?;
                let child_path = crate::storage::path::child(base.path(), path);
                Location::new(base.bucket(), child_path)
            }
            None => self.bucket().ok_or_else(no_default_bucket)?,
        };
        Ok(StorageReference::new(self.clone(), location))
    }
}

fn extract_bucket(host: &str, app: &FirebaseApp) -> StorageResult<Option<Location>> {
    let options = app.options();
    match options.storage_bucket {
        Some(bucket) => Ok(Some(Location::from_bucket_spec(&bucket, host)?)),
        None => Ok(None),
    }
}
