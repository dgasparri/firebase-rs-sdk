use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::app::FirebaseApp;
use crate::app_check::FirebaseAppCheckInternal;
use crate::auth::Auth;
use crate::component::Provider;
use crate::storage::constants::{
    DEFAULT_HOST, DEFAULT_MAX_OPERATION_RETRY_TIME_MS, DEFAULT_MAX_UPLOAD_RETRY_TIME_MS, DEFAULT_PROTOCOL,
};
use crate::storage::error::{internal_error, no_default_bucket, StorageResult};
use crate::storage::location::Location;
use crate::storage::reference::StorageReference;
#[cfg(not(target_arch = "wasm32"))]
use crate::storage::request::StreamingResponse;
use crate::storage::request::{BackoffConfig, HttpClient, RequestInfo};
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

    pub fn connect_emulator(&self, host: &str, port: u16, mock_user_token: Option<String>) -> StorageResult<()> {
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

    pub fn http_client(&self) -> StorageResult<HttpClient> {
        let timeout = Duration::from_millis(self.max_operation_retry_time());
        let config = BackoffConfig::standard_operation().with_total_timeout(timeout);
        HttpClient::new(self.is_using_emulator(), config)
    }

    pub fn upload_http_client(&self) -> StorageResult<HttpClient> {
        let timeout = Duration::from_millis(self.max_upload_retry_time());
        let config = BackoffConfig::upload_operation(timeout);
        HttpClient::new(self.is_using_emulator(), config)
    }

    pub async fn run_request<O>(&self, info: RequestInfo<O>) -> StorageResult<O> {
        let client = self.http_client()?;
        let info = self.prepare_request(info).await?;
        client.execute(info).await
    }

    pub async fn run_upload_request<O>(&self, info: RequestInfo<O>) -> StorageResult<O> {
        let client = self.upload_http_client()?;
        let info = self.prepare_request(info).await?;
        client.execute(info).await
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn run_streaming_request<O>(&self, info: RequestInfo<O>) -> StorageResult<StreamingResponse> {
        let client = self.http_client()?;
        let info = self.prepare_request(info).await?;
        client.execute_streaming(info).await
    }

    async fn prepare_request<O>(&self, mut info: RequestInfo<O>) -> StorageResult<RequestInfo<O>> {
        if let Some(token) = self.auth_token().await? {
            if !token.is_empty() {
                info.headers
                    .insert("Authorization".to_string(), format!("Firebase {token}"));
            }
        }

        if let Some(headers) = self.app_check_headers().await? {
            if !headers.token.is_empty() {
                info.headers.insert("X-Firebase-AppCheck".to_string(), headers.token);
            }
            if let Some(heartbeat) = headers.heartbeat {
                if !heartbeat.is_empty() {
                    info.headers.insert("X-Firebase-Client".to_string(), heartbeat);
                }
            }
        }

        if !info.headers.contains_key("X-Firebase-Storage-Version") {
            let version = format!("webjs/{}", self.firebase_version.as_deref().unwrap_or("AppManager"));
            info.headers.insert("X-Firebase-Storage-Version".to_string(), version);
        }

        if let Some(app_id) = self.app.options().app_id {
            if !app_id.is_empty() {
                info.headers.entry("X-Firebase-GMPID".to_string()).or_insert(app_id);
            }
        }

        Ok(info)
    }

    async fn auth_token(&self) -> StorageResult<Option<String>> {
        if let Some(token) = {
            let state = self.state.lock().unwrap();
            state.override_auth_token.clone()
        } {
            return Ok(Some(token));
        }

        let auth = match self.auth_provider.get_immediate_with_options::<Auth>(None, true) {
            Ok(Some(auth)) => auth,
            Ok(None) => return Ok(None),
            Err(err) => return Err(internal_error(format!("failed to resolve auth provider: {err}"))),
        };

        match auth.get_token(false).await {
            Ok(Some(token)) if token.is_empty() => Ok(None),
            Ok(Some(token)) => Ok(Some(token)),
            Ok(None) => Ok(None),
            Err(err) => Err(internal_error(format!("failed to obtain auth token: {err}"))),
        }
    }

    async fn app_check_headers(&self) -> StorageResult<Option<AppCheckHeaders>> {
        let app_check = match self
            .app_check_provider
            .get_immediate_with_options::<FirebaseAppCheckInternal>(None, true)
        {
            Ok(Some(app_check)) => app_check,
            Ok(None) => return Ok(None),
            Err(err) => return Err(internal_error(format!("failed to resolve app check provider: {err}"))),
        };

        let token = match app_check.get_token(false).await {
            Ok(result) => result.token,
            Err(err) => {
                if let Some(cached) = err.cached_token() {
                    cached.token.clone()
                } else {
                    return Err(internal_error(format!("failed to obtain App Check token: {err}")));
                }
            }
        };

        if token.is_empty() {
            Ok(None)
        } else {
            let heartbeat = app_check
                .heartbeat_header()
                .await
                .map_err(|err| internal_error(format!("failed to obtain App Check heartbeat header: {err}")))?;

            Ok(Some(AppCheckHeaders { token, heartbeat }))
        }
    }
}

struct AppCheckHeaders {
    token: String,
    heartbeat: Option<String>,
}

fn extract_bucket(host: &str, app: &FirebaseApp) -> StorageResult<Option<Location>> {
    let options = app.options();
    match options.storage_bucket {
        Some(bucket) => Ok(Some(Location::from_bucket_spec(&bucket, host)?)),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use crate::app_check::{
        box_app_check_future, AppCheckOptions, AppCheckProvider, AppCheckProviderFuture, AppCheckToken,
    };
    use crate::app_check::{clear_registry, clear_state_for_tests, initialize_app_check, test_guard, token_with_ttl};
    use crate::component::types::{ComponentError, DynService, InstanceFactoryOptions};
    use crate::component::{Component, ComponentType};
    use crate::storage::request::{RequestInfo, ResponseHandler};
    use reqwest::Method;
    use std::future::Future;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    fn unique_settings(prefix: &str) -> FirebaseAppSettings {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!("{prefix}-{}", COUNTER.fetch_add(1, Ordering::SeqCst))),
            ..Default::default()
        }
    }

    fn base_options() -> FirebaseOptions {
        FirebaseOptions {
            storage_bucket: Some("my-bucket".into()),
            app_id: Some("1:123:web:abc".into()),
            ..Default::default()
        }
    }

    fn test_request() -> RequestInfo<()> {
        let handler: ResponseHandler<()> = Arc::new(|_| Ok(()));
        RequestInfo::new("https://example.com", Method::GET, Duration::from_secs(5), handler)
    }

    async fn build_storage_with<F, Fut>(configure: F) -> FirebaseStorageImpl
    where
        F: Fn(&FirebaseApp) -> Fut,
        Fut: Future<Output = ()>,
    {
        let app = initialize_app(base_options(), Some(unique_settings("storage-service")))
            .await
            .expect("failed to initialize app");
        configure(&app).await;

        let container = app.container();
        let auth_provider = container.get_provider("auth-internal");
        let app_check_provider = container.get_provider("app-check-internal");
        FirebaseStorageImpl::new(app, auth_provider, app_check_provider, None, Some("test-sdk".into()))
            .expect("storage construction should succeed")
    }

    #[tokio::test]
    async fn prepare_request_adds_headers_for_emulator_override() {
        let storage = build_storage_with(|_| async {}).await;
        storage
            .connect_emulator("localhost", 9199, Some("mock-token".into()))
            .unwrap();

        let prepared = storage.prepare_request(test_request()).await.unwrap();

        assert_eq!(prepared.headers.get("Authorization"), Some(&"Firebase mock-token".to_string()));

        let expected_version = format!("webjs/{}", storage.firebase_version().unwrap_or("AppManager"));
        assert_eq!(prepared.headers.get("X-Firebase-Storage-Version"), Some(&expected_version));

        assert_eq!(prepared.headers.get("X-Firebase-GMPID"), Some(&"1:123:web:abc".to_string()));

        assert!(prepared.headers.get("X-Firebase-AppCheck").is_none());
    }

    #[derive(Clone)]
    struct StaticAppCheckProvider;

    impl AppCheckProvider for StaticAppCheckProvider {
        fn get_token(&self) -> AppCheckProviderFuture<'_, crate::app_check::AppCheckResult<AppCheckToken>> {
            box_app_check_future(async { token_with_ttl("app-check-token", Duration::from_secs(60)) })
        }
    }

    async fn register_app_check(app: &FirebaseApp) {
        let provider = Arc::new(StaticAppCheckProvider);
        let options = AppCheckOptions::new(provider);
        let app_check = initialize_app_check(Some(app.clone()), options)
            .await
            .expect("initialize app check");
        let internal = Arc::new(FirebaseAppCheckInternal::new(app_check));

        let factory = {
            let internal = internal.clone();
            Arc::new(
                move |_: &crate::component::ComponentContainer,
                      _: InstanceFactoryOptions|
                      -> Result<DynService, ComponentError> { Ok(internal.clone() as DynService) },
            )
        };

        let component = Component::new("app-check-internal", factory, ComponentType::Private);
        app.container().add_or_overwrite_component(component);
    }

    #[tokio::test]
    async fn prepare_request_includes_app_check_header_when_available() {
        let _guard = test_guard();
        clear_state_for_tests();
        clear_registry();
        let storage = build_storage_with(|app| {
            let app = app.clone();
            async move { register_app_check(&app).await }
        })
        .await;
        let prepared = storage.prepare_request(test_request()).await.unwrap();

        assert_eq!(
            prepared.headers.get("X-Firebase-AppCheck"),
            Some(&"app-check-token".to_string())
        );

        clear_state_for_tests();
        clear_registry();
    }
}
