# Firebase Remote Config

This module is the Rust port of the Firebase Remote Config SDK. It exposes a configurable cache of key/value pairs that
can be fetched from the Remote Config backend and activated inside a Firebase app. The current implementation offers an
in-memory stub that wires the component into the shared container so other services can depend on it.

Porting status: 25% `[###       ]` ([details](https://github.com/dgasparri/firebase-rs-sdk/blob/main/src/remote_config/PORTING_STATUS.md))

## Quick Start Example

```rust,no_run
use std::collections::HashMap;

use firebase_rs_sdk::app::initialize_app;
use firebase_rs_sdk::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::remote_config::get_remote_config;

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
    let remote_config = get_remote_config(Some(app.clone())).await?;

    remote_config.set_defaults(HashMap::from([(String::from("welcome"), String::from("hello"))]));
    if remote_config.fetch_and_activate().await? {
        println!("Activated freshly downloaded parameters");
    }

    println!("welcome = {}", remote_config.get_string("welcome"));
    Ok(())
}
```

## References to the Firebase JS SDK

- QuickStart: <https://firebase.google.com/docs/remote-config/get-started?platform=web>
- API: <https://firebase.google.com/docs/reference/js/remote-config.md#remote-config_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/remote-config>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/remote-config>

### WASM Notes
- The module compiles for wasm targets when the `wasm-web` feature is enabled. The default fetch client uses
  [`WasmRemoteConfigFetchClient`](crate::remote_config::fetch::WasmRemoteConfigFetchClient) to perform real fetch operations
  via the browser `fetch` API together with Installations tokens.
- When both `wasm-web` and `experimental-indexed-db` are enabled, Remote Config persists active templates, metadata, and
  custom signals into IndexedDB, mirroring the JS SDK’s storage behaviour across tabs and reloads.

