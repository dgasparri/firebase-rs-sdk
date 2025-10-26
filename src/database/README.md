# Firebase Realtime Database module

This module ports core pieces of the Realtime Database from the Firebase JS SDK to Rust.

It wires the Database component into the `FirebaseApp`, provides an in-memory backend for quick tests, and can fall back to the REST API for basic reads and writes against an emulator or hosted backend. 

Live streaming connections and the richer reference/query surface from the JS SDK are still pending.

It includes error handling, configuration options, and integration with Firebase apps.

## Features

- Component registration and shared get_database resolution
- Reference CRUD with auto-ID push and path navigation (parent/root)
- Priority-aware writes plus server value helpers (server_timestamp increment)
- Snapshot traversal (child, has_child, size, to_json) and value/child listeners
- Dual backends (in-memory + REST) with unit test coverage


## Porting status

- database 30% `[###       ]`

==As of October 21th, 2025== 

Prompt: Compare the original JS/Typescript files in ./packages/database and the ported files in Rust in ./src/database, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust for this module

I'll put together a clear bullet list showing about 30% of the JavaScript Realtime Database features appear ported to Rust, backing it with code references for implemented and missing parts, highlighting key TypeScript modules not yet ported, and finishing with suggestions like verifying features or updating documentation.

Port Estimate

  - Roughly 30 % of the packages/database TypeScript surface is reflected in Rust; the port covers basic Database/
  Reference CRUD, query builders, server values, and token-aware REST + in-memory backends.
  - Implemented pieces include the Database component wiring and backend selection (src/database/api.rs:1499-1522, src/
  database/backend.rs:15-205), core reference/query operations with listener dispatch (src/database/api.rs:773-960,
  src/database/api.rs:1078-1250), snapshot helpers (src/database/api.rs:140-210), push-ID generation, and server value
  helpers (src/database/server_value.rs:1-18), giving functional parity for simple reads/writes and REST filtering.
  - Large swaths of the JS SDK remain unported: OnDisconnect actions error out (src/database/on_disconnect.rs:20-
  50), transactions are stubbed (src/database/api.rs:905-913), and all realtime transports/Repo orchestration are
  placeholders (src/database/realtime.rs:3-56), so anything depending on core/Repo.ts, PersistentConnection.ts, or
  Transaction.ts is missing.
  - Listener support is limited to local diffing; there is no remote event streaming (repo/SyncTree logic from packages/
  database/src/core) or child-moved/query child events, nor off/once/isEqual/toString parity for QueryImpl/ReferenceImpl
  (see packages/database/src/api/Reference_impl.ts for the unported surface).
  - Auxiliary controls such as goOnline/goOffline, emulator toggles, logging, connection stats, validation utilities,
  and Node-specific entry points (packages/database/src/api/Database.ts, core/util) are absent, indicating the broader
  environment/runtime features are still outstanding.

Next steps you might consider:

  1. Prioritise porting the Repo/persistent connection stack to unlock realtime listeners, transactions, and onDisconnect semantics.
  2. Flesh out the remaining public API surface (query equality/serialization helpers, off/once, refFromURL, logging/ emulator controls) and update src/database/README.md once significant pieces land.


## References to the Firebase JS SDK - firestore module

- QuickStart: <https://firebase.google.com/docs/database/web/start>
- API: <https://firebase.google.com/docs/reference/js/database.md#database_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/database>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/database>

## Development status as of 14th October 2025

- Core functionalities: Mostly implemented (see the module's [README.md](https://github.com/dgasparri/firebase-rs-sdk/tree/main/src/firestore) for details)
- Testing: 30 tests (passed)
- Documentation: Most public functions are documented
- Examples: 2 examples

DISCLAIMER: This is not an official Firebase product, nor it is guaranteed that it has no bugs or that it will work as intended.


## Quick Start Example

```rust
use firebase_rs_sdk::app::*;
use firebase_rs_sdk::database::{*, query as compose_query};

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

    let recent = compose_query(
        messages,
        vec![order_by_child("timestamp"), limit_to_last(10)],
    )?;
    let latest = recent.get()?;
    println!("latest snapshot: {latest}");

    Ok(())
}
```

## Current State

- Database component registration via `register_database_component` so `get_database` resolves out of the shared `FirebaseApp` registry.
- Core reference operations (`reference`, `child`, `set`, `update`, `remove`, `get`) that work against any backend and emit `database/invalid-argument` errors for unsupported paths.
- Auto-ID child creation via `DatabaseReference::push()` / `push_with_value()` and the modular `push()` helper, mirroring the JS SDK's append semantics.
- Priority-aware writes through `DatabaseReference::set_with_priority()` / `set_priority()` (and modular helpers), persisting `.value`/`.priority` metadata compatible with REST `format=export`.
- Server value helpers (`server_timestamp`, `increment`) with local resolution for timestamp and atomic increment placeholders across `set`/`update`.
- Child event listeners (`on_child_added`, `on_child_changed`, `on_child_removed`) with in-memory diffing and snapshot traversal utilities for callback parity with the JS SDK.
- Hierarchical navigation APIs (`DatabaseReference::parent/root`) and snapshot helpers (`child`, `has_child`, `has_children`, `size`, `to_json`) that mirror the JS `DataSnapshot` traversal utilities.
- Query builder helpers (`query`, `order_by_*`, `start_*`, `end_*`, `limit_*`, `equal_to*`) with `DatabaseQuery::get()` and REST parameter serialisation.
- `on_value` listeners for references and queries that deliver an initial snapshot and replay callbacks after local writes, returning `ListenerRegistration` handles for manual detach.
- Backend selection that defaults to an in-memory store and upgrades to a REST backend (`reqwest` PUT/PATCH/DELETE/GET) including base query propagation plus optional Auth/App Check token injection.
- Unit tests covering in-memory semantics, validation edge cases, and REST request wiring through `httpmock`.
- Preliminary realtime hooks (`Database::go_online`/`go_offline`) backed by a platform-aware transport selector. The Rust port now normalises listen specs, reference-counts active listeners, and—on native targets—establishes an async WebSocket session using `tokio-tungstenite`, forwarding auth/App Check tokens and queuing listen/unlisten envelopes until the full persistent connection protocol is ported. Streaming payload handling is still pending.

### WASM Notes

- The module compiles on wasm targets when the `wasm-web` feature is enabled. At the moment only the in-memory backend is available on wasm; the REST transport remains native-only until the async client is ported.
- Calling `get_database(None)` is not supported on wasm because the default app lookup is asynchronous. Pass an explicit `FirebaseApp` instance instead.
- `go_online`/`go_offline` are currently stubs on wasm (and native) but provide the async surface needed for upcoming realtime work.

## Next Steps

- Real-time transports (`Repo`, `PersistentConnection`, `WebSocketConnection`, `BrowserPollConnection`) so `onValue`/child events react to remote changes.
- Child event parity: `on_child_moved`, query-level child listeners, and server-ordered `prev_name` semantics from `core/SyncTree.ts`.
- Transactions (`runTransaction`) and `OnDisconnect` execution, including offline queue handling and server timestamp resolution (`Transaction.ts`, `OnDisconnect.ts`).
- Operational controls such as `connectDatabaseEmulator`, `goOnline/goOffline`, and logging toggles from `Database.ts`, plus emulator-focused integration tests.

### Immediate Porting Focus

1. **Child listener parity** – Port the remaining event registrations (`onChildMoved`, query listeners, cancellation hooks) from `Reference_impl.ts` and `SyncTree.ts`, reusing the new diffing infrastructure.
2. **Realtime transport handshake** – Wire the `realtime::Repo` listener map into a full `PersistentConnection` port that sends listen/unlisten commands over the new native websocket task, surfaces errors, and forwards payloads into `dispatch_listeners` (mirrored on wasm via `web_sys::WebSocket`).
3. **Transactions and OnDisconnect** – Replace the temporary stubs with full implementations that queue writes locally, honour abort semantics, and execute deferred mutations when the transport reconnects.
