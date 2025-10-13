# Firestore Port (Rust)

This directory hosts the early-stage Rust port of the Firebase Firestore SDK. The goal is to mirror the JavaScript
implementation while exposing idiomatic Rust APIs that interoperate with the shared component/app infrastructure already
present in this repository.

## Current State

- **Component wiring** – `firestore::api::register_firestore_component` hooks Firestore into the global component
  registry so apps can lazily resolve `Firestore` instances via `get_firestore` (with per-database overrides).
- **Model layer** – Core path primitives (`ResourcePath`, `FieldPath`, `DocumentKey`, `DatabaseId`) and basic value
  types (`FirestoreValue`, `ArrayValue`, `MapValue`, `BytesValue`, `Timestamp`, `GeoPoint`) are ported with unit tests.
- **Minimal references** – `CollectionReference`/`DocumentReference` mirror the modular JS API, including input
  validation and auto-ID generation.
- **Document façade stub** – A simple in-memory datastore backs `FirestoreClient::get_doc/set_doc/add_doc`, exercising
  the value encoding pipeline and providing scaffolding for future network integration.
- **Network scaffolding** – A retrying `HttpDatastore` now wraps the Firestore REST endpoints with JSON
  serialization/deserialization, HTTP error mapping, and pluggable auth/App Check token providers. Auth and App Check
  bridges now feed the HTTP client, including token invalidation/retry on `Unauthenticated` responses.
- **App Check bridge** – `FirebaseAppCheckInternal::token_provider()` exposes App Check credentials as a Firestore
  `TokenProvider`, making it straightforward to attach App Check headers when configuring the HTTP datastore.
- **Typed converters** – Collection and document references accept `with_converter(...)`, and typed snapshots expose
  converter-aware `data()` helpers that match the JS modular API.
- **Snapshot metadata** – `DocumentSnapshot` now carries `SnapshotMetadata`, exposing `from_cache` and
  `has_pending_writes` flags compatible with the JS API.

This is enough to explore API ergonomics and stand up tests, but it lacks real network, persistence, query logic, and the
majority of the Firestore feature set.

## Next Steps

1. **Network layer**
   - Port the remote datastore to call Firestore’s REST/gRPC endpoints (authentication headers, request/response
     serialization, retry/backoff).
   - Handle stream-specific behaviour (listen/write pipelines) once basic REST calls succeed.
2. **Snapshot & converter parity**
   - Flesh out `DocumentSnapshot`, `QuerySnapshot`, and user data converters to match the JS SDK, including `withConverter`
     support and typed accessors.
3. **Write operations**
   - Implement `set_doc`, `update_doc`, `delete_doc`, `write_batch`, and `run_transaction` semantics with proper merge
     options and preconditions.
4. **Query engine**
   - Port query builders (filters, orderBy, limit, cursors) and result handling, then connect them to the remote listen
     stream.
5. **Local persistence**
   - Introduce the local cache layers (memory, IndexedDB-like, LRU pruning) and multi-tab coordination, matching the JS
     architecture.
6. **Bundles & aggregates**
   - Port bundle loading, named queries, and aggregate functions once core querying is functional.
7. **Platform-specific plumbing**
   - Mirror browser/node differences (e.g., persistence availability checks, WebChannel vs gRPC) via `platform/` modules.
8. **Testing & parity checks**
   - Translate the TS unit/integration tests, add coverage for conversions and query logic, and validate against Firebase
     emulators where possible.

## Immediate Porting Focus

| Priority | JS source | Target Rust module | Scope | Key dependencies |
|----------|-----------|--------------------|-------|------------------|
| P0 | `packages/firestore/src/remote/datastore.ts`, `remote/persistent_stream.ts` | Extend `src/firestore/remote/datastore/http.rs`, add `listen`/`write` streaming scaffolding | Layer real listen/write streaming on top of the HTTP bridge (or gRPC when ready), reuse retry/backoff, and surface structured responses. | Needs auth/App Check providers returning real tokens plus async stream abstraction. |
| P0 | `packages/firestore/src/remote/datastore.ts` (token handling) | `src/firestore/remote/datastore/mod.rs`, `http.rs` | ✅ Auth/App Check token providers now feed the HTTP datastore with automatic retry on `Unauthenticated`; still need emulator headers and richer refresh flows. | Depends on porting credential providers from `packages/firestore/src/api/credentials.ts` and wiring to existing `crate::auth`. |
| P0 | `packages/firestore/src/api/snapshot.ts`, `api/reference_impl.ts`, `core/firestore_client.ts` | Split `src/firestore/api/operations.rs` into `snapshot/` modules with converter support | Add typed metadata flags, `with_converter`, and map the HTTP responses to rich snapshots. | Requires serializer parity, encoded reference paths, and converter traits. |
| P1 | `packages/firestore/src/api/transaction.ts`, `api/write_batch.ts`, `core/transaction_runner.ts` | New `src/firestore/api/write.rs`, `src/firestore/core/transaction.rs` | Implement mutations/batches/transactions with merge and precondition semantics against the HTTP datastore. | Builds on serializer support for mutations plus `CommitRequest`/`Rollback` RPCs. |
| P1 | `packages/firestore/src/api/filter.ts`, `core/query.ts`, `core/view.ts`, `remote/watch_change.ts` | `src/firestore/api/query.rs`, `src/firestore/core/query/` | Port query builders, bounds, ordering, and attach them to real listen results. | Requires streaming remote store, target serialization, and comparator logic. |
| P2 | `packages/firestore/src/local/indexeddb_*`, `local/persistence.ts`, `local/remote_document_cache.ts` | `src/firestore/local/` | Establish trait-based persistence with in-memory baseline, paving the way for disk-backed stores later. | Requires query engine integration and watch pipeline parity. |

Follow this order so that the network-backed datastore unblocks richer snapshots and transactional APIs before layering on
query/watch streaming and persistence. As each Rust module solidifies, port the matching TS unit tests from
`packages/firestore/test/unit/` to ensure behavioural parity.

Working through these steps in order will gradually move the port from a stubbed implementation to a fully compatible
Firestore client suitable for real workloads.

## Example Usage

```rust
use firebase-rs-sdk-unofficial-porting::app::api::initialize_app;
use firebase-rs-sdk-unofficial-porting::app::{FirebaseAppSettings, FirebaseOptions};
use firebase-rs-sdk-unofficial-porting::app_check::api::{custom_provider, initialize_app_check, token_with_ttl};
use firebase-rs-sdk-unofficial-porting::app_check::{AppCheckOptions, FirebaseAppCheckInternal};
use firebase-rs-sdk-unofficial-porting::auth::api::auth_for_app;
use firebase-rs-sdk-unofficial-porting::firestore::api::{get_firestore, FirestoreClient};
use firebase-rs-sdk-unofficial-porting::firestore::value::FirestoreValue;
use std::collections::BTreeMap;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // TODO: replace with your project configuration
    let options = FirebaseOptions {
        project_id: Some("your-project".into()),
        // add other Firebase options as needed
        ..Default::default()
    };
    let app = initialize_app(options, Some(FirebaseAppSettings::default()))?;
    let auth = auth_for_app(app.clone())?;

    // Optional: wire App Check tokens into Firestore.
    let app_check_provider = custom_provider(|| token_with_ttl("fake-token", Duration::from_secs(60)));
    let app_check = initialize_app_check(Some(app.clone()), AppCheckOptions::new(app_check_provider))?;
    let app_check_internal = FirebaseAppCheckInternal::new(app_check);

    let firestore = get_firestore(Some(app.clone()))?;
    let client = FirestoreClient::with_http_datastore_authenticated(
        firebase-rs-sdk-unofficial-porting::firestore::api::Firestore::from_arc(firestore.clone()),
        auth.token_provider(),
        Some(app_check_internal.token_provider()),
    )?;

    let mut ada = BTreeMap::new();
    ada.insert("first".into(), FirestoreValue::from_string("Ada"));
    ada.insert("last".into(), FirestoreValue::from_string("Lovelace"));
    ada.insert("born".into(), FirestoreValue::from_integer(1815));
    let ada_snapshot = client.add_doc("users", ada)?;
    println!("Document written with ID: {}", ada_snapshot.id());

    let mut alan = BTreeMap::new();
    alan.insert("first".into(), FirestoreValue::from_string("Alan"));
    alan.insert("middle".into(), FirestoreValue::from_string("Mathison"));
    alan.insert("last".into(), FirestoreValue::from_string("Turing"));
    alan.insert("born".into(), FirestoreValue::from_integer(1912));
    let alan_snapshot = client.add_doc("users", alan)?;
    println!("Document written with ID: {}", alan_snapshot.id());

    Ok(())
}
```

If App Check is not enabled for your app, pass `None` as the third argument to
`with_http_datastore_authenticated`.

### Using Converters

```rust
#[derive(Clone)]
struct UserConverter;

impl FirestoreDataConverter for UserConverter {
    type Model = User;

    fn to_map(
        &self,
        value: &Self::Model,
    ) -> FirestoreResult<BTreeMap<String, FirestoreValue>> {
        // Encode your model into Firestore fields.
        todo!()
    }

    fn from_map(&self, value: &MapValue) -> FirestoreResult<Self::Model> {
        // Decode Firestore fields into your model.
        todo!()
    }
}

let users = firestore.collection("typed-users")?.with_converter(UserConverter);
let doc = users.doc(Some("ada"))?;
client.set_doc_with_converter(&doc, User::new("Ada"), None)?;
let typed_snapshot = client.get_doc_with_converter(&doc)?;
let user: Option<User> = typed_snapshot.data()?;
```
