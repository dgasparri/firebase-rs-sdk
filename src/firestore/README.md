# Firebase Firestore module

This module ports core pieces of the Firestore SDK to Rust so applications 
can discover collections and create, update, retrieve and delete documents.

It provides functionality to interact with Firestore, including retrieving and querying documents,
working with collections, and managing real-time updates. 

It includes error handling, configuration options, and integration with Firebase apps.

## Features

- Connect to Firestore emulator
- Get Firestore instance for a Firebase app
- Register Firestore component
- Manage collections and documents
- Build and execute queries
- Comprehensive error handling

## Porting status

- firestore 25% \[###       \]

==As of October 21th, 2025== 

Prompt: Compare the original JS/Typescript files in ./packages/firestore and the ported files in Rust in ./src/firestore, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust for this module


 Roughly 25 % of the Firestore JS SDK has a Rust counterpart today. 
  - Core handles (app/component registration,
  document/collection references, basic queries, HTTP datastore) exist, but most higher-level features remain unported.
  - Implemented foundations mirror the JS model/value layers and basic client APIs: Firestore registration and handles
  (src/firestore/api/database.rs:20), document and collection references plus typed converters (src/firestore/api/
  reference.rs:17), document/query operations (src/firestore/api/document.rs:20), structured query building with
  cursors (src/firestore/api/query.rs:185), HTTP-backed get/set/runQuery with retry/token wiring (src/firestore/remote/
  datastore/http.rs:60), and JSON serialization utilities (src/firestore/remote/serializer.rs).
  - Key gaps are everywhere the JS SDK leans on the core sync engine: real-time listeners, snapshot observers, pending-
  write coordination, and client write pipeline described in packages/firestore/src/api/reference_impl.ts:1084 and
  orchestrated through packages/firestore/src/core/firestore_client.ts:75 are absent on the Rust side; only polling-
  style reads exist.
  - Mutations beyond plain set/add, batched writes, and transactions defined in packages/firestore/src/api/
  write_batch.ts:37 and packages/firestore/src/api/transaction.ts:103 are not implemented—Rust lacks equivalents to the
  mutation queue, transaction runner, and precondition handling.
  - Offline persistence, cache sizing, multi-tab coordination, and bundle/named-query support (packages/firestore/src/
  api/database.ts:329, packages/firestore/src/local/indexeddb_persistence.ts:369, packages/firestore/src/api/bundle.ts)
  have no Rust analogs; there is just an in-memory stub datastore.
  - Advanced query operators (array-contains, in, not-in) are still rejected (src/firestore/api/query.rs:199), and
  aggregation/bundle APIs (packages/firestore/src/api/aggregate.ts, packages/firestore/src/api/bundle.ts) haven’t been
  ported, highlighting further functional gaps.
  - The scope difference is also reflected in code volume (269 TypeScript sources under packages/firestore/src versus 32
  Rust files under src/firestore), reinforcing that only the initial surface area is available in Rust.

  1. Next natural steps: extend FirestoreClient with update_doc/delete_doc parity and write batches, then tackle real-
  time listeners to unlock broader API coverage.

## References to the Firebase JS SDK - firestore module

- QuickStart: <https://firebase.google.com/docs/firestore/quickstart>
- API: <https://firebase.google.com/docs/reference/js/firestore.md#firestore_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firestore>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/firestore>

## Development status as of 14th October 2025

- Core functionalities: Mostly implemented (see the module's [README.md](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/firestore) for details)
- Testing: 31 tests (passed)
- Documentation: Most public functions are documented
- Examples: 3 examples

DISCLAIMER: This is not an official Firebase product, nor it is guaranteed that it has no bugs or that it will work as intended.

## Quick Start Example

```rust
use firebase_rs_sdk_unofficial::app::*;
use firebase_rs_sdk_unofficial::app_check::*;
use firebase_rs_sdk_unofficial::auth::*;
use firebase_rs_sdk_unofficial::firestore::*;

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
        firebase_rs_sdk_unofficial::firestore::api::Firestore::from_arc(firestore.clone()),
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
use firebase_rs_sdk_unofficial::app::*;
use firebase_rs_sdk_unofficial::firestore::*;
use std::collections::BTreeMap;
#[derive(Clone)]
struct MyUser {
   name: String,
}

#[derive(Clone)]
struct UserConverter;
impl FirestoreDataConverter for UserConverter {
    type Model = MyUser;
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

fn example_with_converter(firestore: &Firestore, client: &FirestoreClient) -> FirestoreResult<Option<MyUser>> {
    let users = firestore.collection("typed-users")?.with_converter(UserConverter);
    let doc = users.doc(Some("ada"))?;
    client.set_doc_with_converter(&doc, MyUser { name: "Ada".to_string() }, None)?;
    let typed_snapshot = client.get_doc_with_converter(&doc)?;
    let user: Option<MyUser> = typed_snapshot.data()?;
    Ok(user)
}
```



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
- **Query API scaffolding** – `Query`, `QuerySnapshot`, and `FirestoreClient::get_docs` now cover collection
  scans as well as `where` filters, `order_by`, `limit`/`limit_to_last`, projections, and cursor bounds when running
  against either the in-memory store or the HTTP datastore (which generates structured queries for Firestore’s REST API).

This is enough to explore API ergonomics and stand up tests, but it lacks real network, persistence, query logic, and the
majority of the Firestore feature set.

## Next Steps

1. **Network layer**
   - Port the remote datastore to call Firestore’s REST/gRPC endpoints (authentication headers, request/response
     serialization, retry/backoff).
   - Handle stream-specific behaviour (listen/write pipelines) once basic REST calls succeed.
   - Extend structured query support with array and membership operators (`array-contains`, `in`, `not-in`), composite
     filters, and collection group queries to complete parity with the modular SDK.
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

### Test Porting Plan

1. **Unit parity pass**
   - Mirror `packages/firestore/test/unit` suites module-by-module (model, value, api, remote, util). Build Rust helpers
     under `src/firestore/test_support` to replace `test/util/helpers.ts` and keep assertions ergonomic.
   - ✅ `model/path` translated (see `tests/firestore/model/resource_path_tests.rs`) alongside supporting helpers.
2. **Converter & snapshot coverage**
   - Port lite/api tests that exercise `withConverter`, snapshot metadata, and reference validation using the in-memory
     datastore.
3. **Serializer & remote edge cases**
   - Translate JSON/proto serializer tests (`test/unit/remote`) and watch/change specs once equivalent Rust modules land.
4. **Local/core engine tests**
   - After local persistence and query engine exist, port `test/unit/local` and `test/unit/core`, reusing generated spec
     JSON fixtures.
5. **Integration staging**
   - Plan emulator-backed integration tests once REST/gRPC networking is wired; gate them behind cargo features to avoid
     CI flakiness.

### Immediate Porting Focus

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
