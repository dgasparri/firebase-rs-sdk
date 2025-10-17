# Firebase Realtime Database Port (Rust)

## Introduction
This module exposes the initial slice of the Rust port of `@firebase/database`.
It wires the Database component into the `FirebaseApp`, provides an in-memory
backend for quick tests, and can fall back to the REST API for basic reads and
writes against an emulator or hosted backend. Live streaming connections and the
richer reference/query surface from the JS SDK are still pending.

## Quick Start Example
```rust
use firebase_rs_sdk_unofficial::app::*;
use firebase_rs_sdk_unofficial::database::{*, query as compose_query};

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

## Implemented
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

## Still to do
- Real-time transports (`Repo`, `PersistentConnection`, `WebSocketConnection`, `BrowserPollConnection`) so `onValue`/child events react to remote changes.
- Child event parity: `on_child_moved`, query-level child listeners, and server-ordered `prev_name` semantics from `core/SyncTree.ts`.
- Transactions (`runTransaction`) and `OnDisconnect` execution, including offline queue handling and server timestamp resolution (`Transaction.ts`, `OnDisconnect.ts`).
- Operational controls such as `connectDatabaseEmulator`, `goOnline/goOffline`, and logging toggles from `Database.ts`, plus emulator-focused integration tests.

## Next Steps - Detailed Completion Plan
1. **Child listener parity** – Port the remaining event registrations (`onChildMoved`, query listeners, cancellation hooks) from `Reference_impl.ts` and `SyncTree.ts`, reusing the new diffing infrastructure.
2. **Realtime transport scaffolding** – Flesh out the new `realtime::Repo` and `PersistentConnection` stubs with channel management, auth handshake, and WebSocket/long-poll adapters inspired by the JS SDK.
3. **Transactions and OnDisconnect** – Replace the temporary stubs with full implementations that queue writes locally, honour abort semantics, and execute deferred mutations when the transport reconnects.
