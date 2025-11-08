# Firebase Data Connect

This module contains the early-stage Rust port of Firebase Data Connect. The goal is to mirror the modular JS SDK
(`@firebase/data-connect`) so applications can execute Data Connect queries asynchronously through the shared component
framework.

Porting status: 5% `[#         ]` ([details](https://github.com/dgasparri/firebase-rs-sdk/blob/main/src/data-connect/PORTING_STATUS.md))

## Quick Start Example

```rust,no_run
use std::collections::BTreeMap;

use firebase_rs_sdk::app::initialize_app;
use firebase_rs_sdk::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::data_connect::{
    get_data_connect_service, QueryRequest
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = FirebaseOptions {
        project_id: Some("demo-project".into()),
        ..Default::default()
    };
    let settings = FirebaseAppSettings {
        name: Some("demo-app".into()),
        ..Default::default()
    };
    let app = initialize_app(options, Some(settings)).await?;
    let service = get_data_connect_service(Some(app.clone()), Some("https://example/graphql"))
        .await?;

    let mut variables = BTreeMap::new();
    variables.insert("id".into(), serde_json::json!(123));

    let response = service
        .execute(QueryRequest {
            operation: "query GetItem($id: ID!) { item(id: $id) { id } }".into(),
            variables,
        })
        .await?;

    println!("response payload: {}", response.data);

    Ok(())
}
```

## References to the Firebase JS SDK

- QuickStart: <https://firebase.google.com/docs/data-connect/quickstart?userflow=automatic#web>
- API: <https://firebase.google.com/docs/reference/js/data-connect.md#data-connect_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/data-connect>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/data-connect>
