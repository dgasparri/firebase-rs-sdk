# Firebase Realtime Database

This module ports core pieces of the Realtime Database from the Firebase JS SDK to Rust.

It wires the Database component into the `FirebaseApp`, provides an in-memory backend for quick tests, and can fall back to the REST API for basic reads and writes against an emulator or hosted backend. 

Live streaming connections and the richer reference/query surface from the JS SDK are still pending.

It includes error handling, configuration options, and integration with Firebase apps.

Porting status: 30% `[###       ]` ([details](https://github.com/dgasparri/firebase-rs-sdk/blob/main/src/database/PORTING_STATUS.md))

## Features

- Component registration and shared get_database resolution
- Reference CRUD with auto-ID push and path navigation (parent/root)
- Priority-aware writes plus server value helpers (server_timestamp increment)
- Snapshot traversal (child, has_child, size, to_json) and value/child listeners
- Dual backends (in-memory + REST) with unit test coverage

## Quick Start Example

```rust,no_run
use firebase_rs_sdk::app::*;
use firebase_rs_sdk::database::{*, query as compose_query};
use serde_json::json;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Point to the Realtime Database emulator or a database URL.
    let options = FirebaseOptions {
        project_id: Some("demo-project".into()),
        database_url: Some("http://127.0.0.1:9000/?ns=demo".into()),
        ..Default::default()
    };
    let app = initialize_app(options, Some(FirebaseAppSettings::default())).await?;
    let database = get_database(Some(app)).await?;

    let messages = database.reference("/messages")?;
    messages.set(json!({ "greeting": "hello" })).await?;
    let value = messages.get().await?;
    assert_eq!(value, json!({ "greeting": "hello" }));

    let recent = compose_query(
        messages,
        vec![order_by_child("timestamp"), limit_to_last(10)],
    )?;
    let latest = recent.get().await?;
    println!("latest snapshot: {latest}");

    Ok(())
}
```

## References to the Firebase JS SDK

- QuickStart: <https://firebase.google.com/docs/database/web/start>
- API: <https://firebase.google.com/docs/reference/js/database.md#database_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/database>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/database>


## WASM Notes

- The module compiles on wasm targets when the `wasm-web` feature is enabled. Web builds attempt to establish a realtime WebSocket and transparently degrade to a long-poll fetch loop when sockets cannot open, reusing the same listener bookkeeping as native builds. `OnDisconnect` operations require an active WebSocket; on the long-poll fallback they are queued locally and executed when `go_offline()` runs, which does not fully replicate server-side disconnect handling.
- Calling `get_database(None)` is not supported on wasm because the default app lookup is asynchronous. Pass an explicit `FirebaseApp` instance instead.
- `go_online`/`go_offline` are currently stubs on wasm (and native) but provide the async surface needed for upcoming realtime work.

