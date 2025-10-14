use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Method;
use serde_json::Value;

use crate::storage::error::internal_error;
use crate::storage::location::Location;
use crate::storage::service::FirebaseStorageImpl;

use super::{RequestInfo, ResponseHandler};

pub fn get_metadata_request(
    storage: &FirebaseStorageImpl,
    location: &Location,
) -> RequestInfo<Value> {
    let base_url = format!("{}/v0{}", storage.host(), location.full_server_url());
    let timeout = Duration::from_millis(storage.max_operation_retry_time());

    let handler: ResponseHandler<Value> = Arc::new(|payload| {
        serde_json::from_slice(&payload.body)
            .map_err(|err| internal_error(format!("failed to parse metadata: {err}")))
    });

    RequestInfo::new(base_url, Method::GET, timeout, handler)
        .with_query_param("alt", "json")
        .with_headers(default_json_headers())
}

fn default_json_headers() -> HashMap<String, String> {
    let mut headers = HashMap::new();
    headers.insert("Accept".to_string(), "application/json".to_string());
    headers
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
                "storage-request-{}",
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
    fn builds_get_metadata_request() {
        let storage = build_storage();
        let location = Location::new("my-bucket", "photos/cat.png");
        let request = get_metadata_request(&storage, &location);
        assert_eq!(
            request.url,
            "firebasestorage.googleapis.com/v0/b/my%2Dbucket/o/photos%2Fcat%2Epng"
        );
        assert_eq!(request.method, Method::GET);
        assert_eq!(request.query_params.get("alt"), Some(&"json".to_string()));
    }
}
