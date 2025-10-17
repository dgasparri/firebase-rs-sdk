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
- Hierarchical navigation APIs (`DatabaseReference::parent/root`) and snapshot helpers (`child`, `has_child`, `has_children`, `size`, `to_json`) that mirror the JS `DataSnapshot` traversal utilities.
- Query builder helpers (`query`, `order_by_*`, `start_*`, `end_*`, `limit_*`, `equal_to*`) with `DatabaseQuery::get()` and REST parameter serialisation.
- `on_value` listeners for references and queries that deliver an initial snapshot and replay callbacks after local writes, returning `ListenerRegistration` handles for manual detach.
- Backend selection that defaults to an in-memory store and upgrades to a REST backend (`reqwest` PUT/PATCH/DELETE/GET) including base query propagation plus optional Auth/App Check token injection.
- Unit tests covering in-memory semantics, validation edge cases, and REST request wiring through `httpmock`.

## Still to do
- Rich `DataSnapshot` and `DatabaseReference` APIs (child iteration, `has_child`, `size`, `parent`, `root`, `is_equal`, `to_string`, JSON export) to match `Reference_impl.ts`.
- Real-time transports (`Repo`, `PersistentConnection`, `WebSocketConnection`, `BrowserPollConnection`) so `onValue`/child events react to remote changes.
- Child event registrations (`on_child_added/changed/moved/removed`) and query cancellation semantics currently implemented in `QueryImpl` & `SyncTree.ts`.
- Transactions (`runTransaction`), `OnDisconnect`, server timestamps, and offline queue handling from `Transaction.ts` and `OnDisconnect.ts`.
- Operational controls such as `connectDatabaseEmulator`, `goOnline/goOffline`, and logging toggles from `Database.ts`, plus emulator-focused integration tests.

## Next Steps - Detailed Completion Plan
1. **Introduce child event listeners** – Extend the listener registry to track event types, surface `on_child_added/changed/moved/removed`, and mirror the ordering logic from `core/SyncTree.ts`; seed with in-memory coverage while stubbing remote hooks.
2. **Realtime transport scaffolding** – Create `Repo` and `PersistentConnection` skeletons inspired by `core/Repo.ts` and `core/PersistentConnection.ts`, routing local writes through the queue and preparing a WebSocket transport behind a feature flag.
3. **Transactions and OnDisconnect** – Port `runTransaction` and `OnDisconnect` support from `Transaction.ts` / `OnDisconnect.ts`, covering optimistic concurrency, abort semantics, and unit/integration tests with server values.
