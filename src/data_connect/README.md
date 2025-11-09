# Firebase Data Connect

Rust port of the Firebase Data Connect SDK. The module mirrors the modular JS surface so apps can register connectors, execute queries or mutations, and hydrate caches both natively and in WASM builds.

Porting status: 80% `[########  ]` ([details](https://github.com/dgasparri/firebase-rs-sdk/blob/main/src/data-connect/PORTING_STATUS.md))

## Quick Start Example

```rust,no_run
use std::sync::Arc;

use firebase_rs_sdk::app::initialize_app;
use firebase_rs_sdk::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::data_connect::{
    connect_data_connect_emulator, execute_query, get_data_connect_service, query_ref, subscribe,
    ConnectorConfig, QuerySubscriptionHandlers,
};
use serde_json::json;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = FirebaseOptions {
        project_id: Some("demo-project".into()),
        api_key: Some("demo-key".into()),
        ..Default::default()
    };
    let settings = FirebaseAppSettings {
        name: Some("demo-app".into()),
        ..Default::default()
    };
    let app = initialize_app(options, Some(settings)).await?;

    let connector = ConnectorConfig::new("us-central1", "books", "catalog")?;
    let service = get_data_connect_service(Some(app.clone()), connector).await?;
    // Optional: route to the local emulator during development.
    connect_data_connect_emulator(&service, "localhost", Some(9399), false)?;

    let query = query_ref(Arc::clone(&service), "ListBooks", json!({"limit": 25}));
    let result = execute_query(&query).await?;
    println!("{}", result.data);

    // Async subscriptions deliver cached data immediately when provided.
    let handlers = QuerySubscriptionHandlers::new(Arc::new(|result| {
        println!("cache update: {}", result.data);
    }));
    let _subscription = subscribe(query, handlers, None).await?;

    Ok(())
}
```

## References to the Firebase JS SDK

- QuickStart: <https://firebase.google.com/docs/data-connect/quickstart?userflow=automatic#web>
- API: <https://firebase.google.com/docs/reference/js/data-connect.md#data-connect_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/data-connect>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/data-connect>

## WASM Notes

- The transport layer uses `reqwest`’s fetch backend when targeting `wasm32-unknown-unknown`, so no additional shims are required.
- `subscribe` and the query/mutation managers avoid `Send` bounds when compiling with the `wasm-web` feature so callbacks can capture browser-only types.
