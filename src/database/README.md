# Firebase Realtime Database Port (Rust)

## Introduction
This module provides the Rust port of `@firebase/database`, exposing the familiar
Realtime Database API on top of Rust components. The current implementation
registers the database component, offers an in-memory data tree for rapid tests,
and now introduces a REST transport pathway so the SDK can speak to an emulator
or backend over HTTPS.

## Quick Start Example
```rust
use firebase_rs_sdk_unofficial::app::api::initialize_app;
use firebase_rs_sdk_unofficial::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk_unofficial::database::api::get_database;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Point to the Realtime Database emulator or a database URL.
    let options = FirebaseOptions {
        project_id: Some("demo-project".into()),
        database_url: Some("http://127.0.0.1:9000/?ns=demo".into()),
        ..Default::default()
    };
    let app = initialize_app(options, Some(FirebaseAppSettings::default()))?;
    let database = get_database(Some(app))?;

    let messages = database.reference("/messages")?;
    messages.set(json!({ "greeting": "hello" }))?;
    let value = messages.get()?;
    assert_eq!(value, json!({ "greeting": "hello" }));

    Ok(())
}
```

## Implemented
- Component registration (`register_database_component`) allows `get_database` to work with any `FirebaseApp`.
- In-memory backend that mirrors the JS SDK stub for fast, offline reads and writes.
- REST transport foundation using `reqwest` that performs `set`/`get` operations when `database_url` is configured.
- Path validation mirroring the JavaScript rules (no empty segments) with error codes aligned to the JS SDK.
- Unit tests covering both in-memory behaviour and REST request wiring (using `httpmock`).

## Still to do
- WebSocket realtime protocol (`Repo`/`Connection` port) for live event streaming (`onValue`, child events, cancellations).
- Authentication and App Check integration for REST/WebSocket requests, including permission error mapping.
- Offline persistence, transaction logic, `onDisconnect`, and server timestamp handling.
- Query operators (`orderBy`, `startAt`, `limitToFirst`, etc.) and indexing helpers.
- Platform adapters mirroring browser/node differences for future WASM support.
- Comprehensive parity tests against the Firebase emulator, ported from `packages/database` and `packages/firebase/database`.

## Next Steps - Detailed Completion Plan
1. **Broaden REST coverage** – Port the request builders from `packages/database/src/realtime/RestClient.ts` to implement `PATCH`, `DELETE`, query parameters, and error mapping; add fixtures mirroring the JS tests.
2. **Token handling** – Integrate Auth/App Check token providers so the REST backend attaches `auth`/`AppCheck` headers; align with `packages/database/src/core/AuthTokenProvider.ts`.
3. **Realtime connection scaffolding** – Introduce structs mirroring `Repo` and `PersistentConnection` to manage WebSocket sessions, event queues, and connection retries; start with a no-op event loop that surfaces `on_value` callbacks.
4. **Persistence layer** – Add a pluggable cache (similar to `ServerActionsQueue` in TS) to stage writes offline and replay them when the connection resumes; gate browser-specific storage behind a `wasm-web` feature.
5. **Test porting** – Begin translating the JS emulator test suites (`packages/database/test/`) to Rust integration tests that run against the Firebase emulator, covering listeners, transactions, and security errors.
