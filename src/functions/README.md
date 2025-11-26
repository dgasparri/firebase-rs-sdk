# Firebase Functions

The `functions` module provides a Rust port of the Firebase Cloud Functions (client) SDK so
applications can invoke HTTPS callable backends from native or WASM targets. The goal is to mirror
the modular JavaScript API (`@firebase/functions`) while using idiomatic Rust primitives.

Porting status: 25% `[###       ]` ([details](https://github.com/dgasparri/firebase-rs-sdk/blob/main/src/functions/PORTING_STATUS.md))

## Quick Start Example

```rust,no_run
use firebase_rs_sdk::app::initialize_app;
use firebase_rs_sdk::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::functions::{get_functions, register_functions_component};
use serde_json::{json, Value as JsonValue};

# #[cfg(target_arch = "wasm32")]
# fn main() {}
#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    register_functions_component();

    let app = initialize_app(
        FirebaseOptions {
            project_id: Some("demo-project".into()),
            ..Default::default()
        },
        Some(FirebaseAppSettings::default()),
    )
    .await?;

    let functions = get_functions(Some(app.clone()), None).await?;
    let callable = functions
        .https_callable::<JsonValue, JsonValue>("helloWorld")?;

    let response = callable
        .call_async(&json!({ "message": "hi" }))
        .await?;
    println!("response: {response:?}");
    Ok(())
}
```

Use your platform's async runtime to drive the `main` future (for example `#[tokio::main]` on
native targets or `wasm_bindgen_futures::spawn_local` on `wasm32-unknown-unknown`).

## References to the Firebase JS SDK

- QuickStart: <https://firebase.google.com/docs/functions/callable#call_the_function>
- API: <https://firebase.google.com/docs/reference/js/functions.md#functions_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/functions>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/functions>
