//! Persist Remote Config data across runs using the file-backed storage helper.
//!
//! This example swaps in a static fetch client so it works offline; replace it with the
//! default HTTP client (by removing `set_fetch_client`) to reach the real backend.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use firebase_rs_sdk::app::initialize_app;
use firebase_rs_sdk::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::remote_config::FileRemoteConfigStorage;
use firebase_rs_sdk::remote_config::RemoteConfig;
use firebase_rs_sdk::remote_config::RemoteConfigResult;
use firebase_rs_sdk::remote_config::{FetchRequest, FetchResponse, RemoteConfigFetchClient};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = FirebaseOptions {
        api_key: Some("AIza_your_api_key".into()),
        project_id: Some("your-project-id".into()),
        app_id: Some("1:1234567890:web:abc123def456".into()),
        ..Default::default()
    };

    let app = initialize_app(options, Some(FirebaseAppSettings::default())).await?;

    let storage_path = PathBuf::from("./remote-config-cache.json");
    let storage = Arc::new(FileRemoteConfigStorage::new(storage_path.clone())?);
    let remote_config = RemoteConfig::with_storage(app.clone(), storage);

    // Stub fetch client avoids network traffic; swap in the default by removing this line.
    remote_config.set_fetch_client(Arc::new(StaticFetchClient));

    // Pull values (from the stub) and persist them to disk so the next run can reload them.
    if remote_config.fetch_and_activate().await? {
        println!("Wrote fresh template to {}", storage_path.display());
    }

    let message = remote_config.get_string("message_of_the_day");
    let source = remote_config.get_value("message_of_the_day").source().as_str();
    println!("message_of_the_day ({source}): {message}");
    println!("Restart the program to reuse cached values from {}", storage_path.display());

    Ok(())
}

struct StaticFetchClient;

#[async_trait]
impl RemoteConfigFetchClient for StaticFetchClient {
    async fn fetch(&self, _request: FetchRequest) -> RemoteConfigResult<FetchResponse> {
        Ok(FetchResponse {
            status: 200,
            etag: Some(String::from("demo-etag")),
            config: Some(HashMap::from([(
                String::from("message_of_the_day"),
                String::from("Hello from cached Remote Config"),
            )])),
            template_version: Some(1),
        })
    }
}
