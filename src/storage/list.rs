use crate::storage::error::{internal_error, StorageResult};
use crate::storage::location::Location;
use crate::storage::reference::StorageReference;
use crate::storage::service::FirebaseStorageImpl;
use serde::Deserialize;

#[derive(Clone, Debug, Default)]
pub struct ListOptions {
    pub max_results: Option<u32>,
    pub page_token: Option<String>,
}

#[derive(Clone, Default)]
pub struct ListResult {
    pub prefixes: Vec<StorageReference>,
    pub items: Vec<StorageReference>,
    pub next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListResponse {
    #[serde(default)]
    prefixes: Vec<String>,
    #[serde(default)]
    items: Vec<ListItem>,
    #[serde(default)]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListItem {
    name: String,
    #[serde(default)]
    bucket: Option<String>,
}

pub fn build_list_options(prefix: &Location, options: &ListOptions) -> Vec<(String, String)> {
    let mut params = Vec::new();
    let prefix_path = if prefix.is_root() {
        "".to_string()
    } else {
        format!("{}/", prefix.path())
    };
    params.push(("prefix".to_string(), prefix_path));
    params.push(("delimiter".to_string(), "/".to_string()));
    if let Some(token) = &options.page_token {
        params.push(("pageToken".to_string(), token.clone()));
    }
    if let Some(max) = options.max_results {
        params.push(("maxResults".to_string(), max.to_string()));
    }
    params
}

pub fn parse_list_result(
    storage: &FirebaseStorageImpl,
    bucket: &str,
    response: serde_json::Value,
) -> StorageResult<ListResult> {
    let parsed: ListResponse = serde_json::from_value(response.clone()).map_err(|err| {
        internal_error(format!("invalid list response: {err}; payload: {response}"))
    })?;

    let mut result = ListResult::default();

    for prefix in parsed.prefixes {
        let trimmed = prefix.trim_end_matches('/');
        let location = Location::new(bucket, trimmed);
        result
            .prefixes
            .push(StorageReference::new(storage.clone(), location));
    }

    for item in parsed.items {
        let item_bucket = item.bucket.unwrap_or_else(|| bucket.to_string());
        let location = Location::new(item_bucket, item.name);
        result
            .items
            .push(StorageReference::new(storage.clone(), location));
    }

    result.next_page_token = parsed.next_page_token;
    Ok(result)
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
                "storage-list-{}",
                COUNTER.fetch_add(1, Ordering::SeqCst)
            )),
            ..Default::default()
        }
    }

    async fn build_storage() -> FirebaseStorageImpl {
        let options = FirebaseOptions {
            storage_bucket: Some("my-bucket".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .unwrap();
        let container = app.container();
        let auth_provider = container.get_provider("auth-internal");
        let app_check_provider = container.get_provider("app-check-internal");
        FirebaseStorageImpl::new(app, auth_provider, app_check_provider, None, None).unwrap()
    }

    #[tokio::test]
    async fn parses_list_response() {
        let storage = build_storage().await;
        let json = serde_json::json!({
            "prefixes": ["photos/"],
            "items": [
                {"name": "photos/cat.jpg"},
                {"name": "photos/dog.jpg"}
            ],
            "nextPageToken": "abc"
        });
        let result = parse_list_result(&storage, "my-bucket", json).unwrap();
        assert_eq!(result.prefixes.len(), 1);
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.next_page_token.as_deref(), Some("abc"));
    }
}
