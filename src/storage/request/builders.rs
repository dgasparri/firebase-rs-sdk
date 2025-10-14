use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Method;
use serde_json::Value;
use url::form_urlencoded;

use crate::storage::error::internal_error;
use crate::storage::list::{build_list_options, ListOptions};
use crate::storage::location::Location;
use crate::storage::service::FirebaseStorageImpl;
use crate::storage::SetMetadataRequest;

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

pub fn update_metadata_request(
    storage: &FirebaseStorageImpl,
    location: &Location,
    metadata: SetMetadataRequest,
) -> RequestInfo<Value> {
    let base_url = format!("{}/v0{}", storage.host(), location.full_server_url());
    let timeout = Duration::from_millis(storage.max_operation_retry_time());

    let handler: ResponseHandler<Value> = Arc::new(|payload| {
        serde_json::from_slice(&payload.body)
            .map_err(|err| internal_error(format!("failed to parse metadata: {err}")))
    });

    RequestInfo::new(base_url, Method::PATCH, timeout, handler)
        .with_query_param("alt", "json")
        .with_headers(default_json_headers())
        .with_body(super::info::RequestBody::Text(
            serde_json::to_string(&metadata).expect("metadata serialization should never fail"),
        ))
}

pub fn list_request(
    storage: &FirebaseStorageImpl,
    location: &Location,
    options: &ListOptions,
) -> RequestInfo<Value> {
    let base_url = format!("{}/v0{}", storage.host(), location.bucket_only_server_url());
    let timeout = Duration::from_millis(storage.max_operation_retry_time());
    let handler: ResponseHandler<Value> = Arc::new(|payload| {
        serde_json::from_slice(&payload.body)
            .map_err(|err| internal_error(format!("failed to parse list response: {err}")))
    });

    let mut request = RequestInfo::new(base_url, Method::GET, timeout, handler)
        .with_query_param("alt", "json")
        .with_headers(default_json_headers());

    for (key, value) in build_list_options(location, options) {
        request = request.with_query_param(key, value);
    }

    request
}

pub fn download_bytes_request(
    storage: &FirebaseStorageImpl,
    location: &Location,
    max_download_size_bytes: Option<u64>,
) -> RequestInfo<Vec<u8>> {
    let base_url = format!("{}/v0{}", storage.host(), location.full_server_url());
    let timeout = Duration::from_millis(storage.max_operation_retry_time());

    let handler: ResponseHandler<Vec<u8>> = Arc::new(|payload| Ok(payload.body));

    let mut request = RequestInfo::new(base_url, Method::GET, timeout, handler);
    request
        .query_params
        .insert("alt".to_string(), "media".to_string());

    if let Some(limit) = max_download_size_bytes {
        request
            .headers
            .insert("Range".to_string(), format!("bytes=0-{limit}"));
        request.success_codes = vec![200, 206];
    }

    request
}

pub fn download_url_request(
    storage: &FirebaseStorageImpl,
    location: &Location,
) -> RequestInfo<Option<String>> {
    let url_part = location.full_server_url();
    let base_url = format!("{}/v0{}", storage.host(), url_part.clone());
    let timeout = Duration::from_millis(storage.max_operation_retry_time());

    let download_base = format!("{}://{}/v0{}", storage.protocol(), storage.host(), url_part);

    let handler: ResponseHandler<Option<String>> = Arc::new(move |payload| {
        let value: Value = serde_json::from_slice(&payload.body)
            .map_err(|err| internal_error(format!("failed to parse download metadata: {err}")))?;

        if let Some(tokens) = value
            .get("downloadTokens")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            if let Some(token) = tokens.split(',').find(|segment| !segment.is_empty()) {
                let encoded_token: String =
                    form_urlencoded::byte_serialize(token.as_bytes()).collect();
                return Ok(Some(format!(
                    "{}?alt=media&token={}",
                    download_base, encoded_token
                )));
            }
        }

        Ok(None)
    });

    let mut request = RequestInfo::new(base_url, Method::GET, timeout, handler);
    request.headers = default_json_headers();
    request
}

pub fn delete_object_request(
    storage: &FirebaseStorageImpl,
    location: &Location,
) -> RequestInfo<()> {
    let base_url = format!("{}/v0{}", storage.host(), location.full_server_url());
    let timeout = Duration::from_millis(storage.max_operation_retry_time());

    let handler: ResponseHandler<()> = Arc::new(|_| Ok(()));

    let mut request = RequestInfo::new(base_url, Method::DELETE, timeout, handler);
    request.success_codes = vec![200, 204];
    request
}

fn default_json_headers() -> HashMap<String, String> {
    let mut headers = HashMap::new();
    headers.insert("Accept".to_string(), "application/json".to_string());
    headers.insert("Content-Type".to_string(), "application/json".to_string());
    headers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::api::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use crate::storage::metadata::SetMetadataRequest;
    use crate::storage::request::{RequestBody, ResponsePayload};
    use reqwest::StatusCode;

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

    #[test]
    fn builds_update_metadata_request() {
        let storage = build_storage();
        let location = Location::new("my-bucket", "docs/file.txt");
        let mut metadata = SetMetadataRequest::default();
        metadata.content_type = Some("text/plain".into());
        let request = update_metadata_request(&storage, &location, metadata);
        assert_eq!(request.method, Method::PATCH);
        assert_eq!(
            request.url,
            "firebasestorage.googleapis.com/v0/b/my%2Dbucket/o/docs%2Ffile%2Etxt"
        );
        assert_eq!(request.query_params.get("alt"), Some(&"json".to_string()));
        match &request.body {
            RequestBody::Text(body) => {
                assert!(body.contains("\"contentType\":\"text/plain\""));
            }
            _ => panic!("expected text body"),
        }
    }

    #[test]
    fn builds_list_request() {
        let storage = build_storage();
        let location = Location::new("my-bucket", "");
        let mut options = ListOptions::default();
        options.max_results = Some(25);
        options.page_token = Some("token123".into());
        let request = list_request(&storage, &location, &options);
        assert_eq!(request.method, Method::GET);
        assert_eq!(
            request.query_params.get("delimiter"),
            Some(&"/".to_string())
        );
        assert_eq!(request.query_params.get("prefix"), Some(&"".to_string()));
        assert_eq!(
            request.query_params.get("maxResults"),
            Some(&"25".to_string())
        );
        assert_eq!(
            request.query_params.get("pageToken"),
            Some(&"token123".to_string())
        );
    }

    #[test]
    fn download_bytes_request_sets_range_header() {
        let storage = build_storage();
        let location = Location::new("my-bucket", "docs/file.txt");
        let request = download_bytes_request(&storage, &location, Some(1024));
        assert_eq!(request.method, Method::GET);
        assert_eq!(request.query_params.get("alt"), Some(&"media".to_string()));
        assert_eq!(
            request.headers.get("Range"),
            Some(&"bytes=0-1024".to_string())
        );
        assert_eq!(request.success_codes, vec![200, 206]);
    }

    #[test]
    fn download_url_request_builds_signed_url() {
        let storage = build_storage();
        let location = Location::new("my-bucket", "photos/cat.jpg");
        let request = download_url_request(&storage, &location);

        let payload = ResponsePayload {
            status: StatusCode::OK,
            headers: HashMap::new(),
            body: serde_json::to_vec(&serde_json::json!({
                "downloadTokens": "token123"
            }))
            .unwrap(),
        };

        let handler = request.response_handler.clone();
        let url = handler(payload).unwrap().unwrap();
        assert!(url.contains("token=token123"));
        assert!(url.starts_with("https://"));
        assert!(url.contains("/v0/b/my%2Dbucket"));
    }

    #[test]
    fn delete_object_request_accepts_empty_response() {
        let storage = build_storage();
        let location = Location::new("my-bucket", "docs/file.txt");
        let request = delete_object_request(&storage, &location);
        assert_eq!(request.method, Method::DELETE);
        assert!(request.success_codes.contains(&204));
    }
}
