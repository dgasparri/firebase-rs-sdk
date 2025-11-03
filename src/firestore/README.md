# Firebase Firestore module

## Introduction

This module ports core pieces of the Firestore SDK to Rust so applications can
discover collections and create, update, retrieve, and delete documents. It
provides functionality to interact with Firestore, including retrieving and
querying documents, working with collections, and managing real-time updates.

It includes error handling, configuration options, and integration with
Firebase apps.

## Porting status

- firestore 60% `[######   ]`

==As of April 5th, 2026==

Roughly 60 % of the Firestore JS SDK now has a Rust counterpart.

- Core handles (component registration, `Firestore`/reference types, converters), document/query operations, and the HTTP datastore remain in place.
- Set semantics now mirror the JS SDK, including `SetOptions::merge`/`merge_fields`, allowing partial document updates through both the HTTP and in-memory datastores.
- All array/membership operators (`array-contains`, `array-contains-any`, `in`, `not-in`) are supported and validated; queries auto-normalise ordering just like the JS structured-query builder.
- Batched writes (`WriteBatch`) are fully wired, sharing the commit pipeline across HTTP and in-memory datastores so atomic groups of set/update/delete mirror `writeBatch()` from the modular SDK.
- Remaining gaps focus on the sync engine: real-time watchers, transactions, precondition/sentinel transforms, offline persistence, and bundle/aggregate support.

## References to the Firebase JS SDK - firestore module

- QuickStart: <https://firebase.google.com/docs/firestore/quickstart>
- API: <https://firebase.google.com/docs/reference/js/firestore.md#firestore_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firestore>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/firestore>

## Development status as of 5th April 2026

- Core functionalities: Mostly implemented (see this README for details)
- Testing: 44 tests (covering merges, advanced filters, batched writes, and existing update/delete flows)
- Documentation: Public APIs documented with rustdoc (set merge semantics, `WriteBatch`, advanced filters)
- Examples: 3 examples

DISCLAIMER: This is not an official Firebase product, nor it is guaranteed that it has no bugs or that it will work as intended.

## Quick Start Example

```rust,ignore
use firebase_rs_sdk::app::*;
use firebase_rs_sdk::app_check::*;
use firebase_rs_sdk::auth::*;
use firebase_rs_sdk::firestore::*;

use std::collections::BTreeMap;
use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // TODO: replace with your project configuration
    let options = FirebaseOptions {
        project_id: Some("your-project".into()),
        // add other Firebase options as needed
        ..Default::default()
    };
    let app = initialize_app(options, Some(FirebaseAppSettings::default())).await?;
    let auth = auth_for_app(app.clone())?;
    // Optional: wire App Check tokens into Firestore.
    let app_check_provider = custom_provider(|| token_with_ttl("fake-token", Duration::from_secs(60)));
    let app_check =
        initialize_app_check(Some(app.clone()), AppCheckOptions::new(app_check_provider)).await?;
    let app_check_internal = FirebaseAppCheckInternal::new(app_check);
    let firestore = get_firestore(Some(app.clone())).await?;
    let client = FirestoreClient::with_http_datastore_authenticated(
        firebase_rs_sdk::firestore::api::Firestore::from_arc(firestore.clone()),
        auth.token_provider(),
        Some(app_check_internal.token_provider()),
    )?;
    let mut ada = BTreeMap::new();
    ada.insert("first".into(), FirestoreValue::from_string("Ada"));
    ada.insert("last".into(), FirestoreValue::from_string("Lovelace"));
    ada.insert("born".into(), FirestoreValue::from_integer(1815));
    let ada_snapshot = client.add_doc("users", ada).await?;
    println!("Document written with ID: {}", ada_snapshot.id());
    let mut alan = BTreeMap::new();
    alan.insert("first".into(), FirestoreValue::from_string("Alan"));
    alan.insert("middle".into(), FirestoreValue::from_string("Mathison"));
    alan.insert("last".into(), FirestoreValue::from_string("Turing"));
    alan.insert("born".into(), FirestoreValue::from_integer(1912));
    let alan_snapshot = client.add_doc("users", alan).await?;
    println!("Document written with ID: {}", alan_snapshot.id());
    Ok(())
}
```

If App Check is not enabled for your app, pass `None` as the third argument to
`with_http_datastore_authenticated`.

### Using Converters

```rust
use firebase_rs_sdk::app::*;
use firebase_rs_sdk::firestore::*;
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

async fn example_with_converter(
    firestore: &Firestore,
    client: &FirestoreClient,
) -> FirestoreResult<Option<MyUser>> {
    let users = firestore.collection("typed-users")?.with_converter(UserConverter);
    let doc = users.doc(Some("ada"))?;
    client
        .set_doc_with_converter(&doc, MyUser { name: "Ada".to_string() }, None)
        .await?;
    let typed_snapshot = client.get_doc_with_converter(&doc).await?;
    let user: Option<MyUser> = typed_snapshot.data()?;
    Ok(user)
}
```



## Implemented

- **Component wiring** – `firestore::api::register_firestore_component` hooks Firestore into the global component
  registry so apps can lazily resolve `Firestore` instances via `get_firestore` (with per-database overrides).
- **Model layer** – Core path primitives (`ResourcePath`, `FieldPath`, `DocumentKey`, `DatabaseId`) and basic value
  types (`FirestoreValue`, `ArrayValue`, `MapValue`, `BytesValue`, `Timestamp`, `GeoPoint`) are ported with unit tests.
- **Minimal references** – `CollectionReference`/`DocumentReference` mirror the modular JS API, including input
  validation and auto-ID generation.
- **Document façade** – A simple in-memory datastore backs
  `FirestoreClient::get_doc/set_doc/add_doc/update_doc/delete_doc`, exercising the value encoding pipeline and
  providing scaffolding for future network integration.
- **Single-document writes** – The HTTP datastore now issues REST commits for `set_doc`, `set_doc_with_converter`,
  `update_doc`, and `delete_doc` with retry/backoff and auth/App Check headers. `SetOptions` supports both
  `merge` and `merge_fields`, matching JS semantics on HTTP and in-memory stores.
- **Network scaffolding** – A retrying `HttpDatastore` now wraps the Firestore REST endpoints with JSON
  serialization/deserialization, HTTP error mapping, and pluggable auth/App Check token providers. Auth and App Check
  bridges now feed the HTTP client, including token invalidation/retry on `Unauthenticated` responses.
- **App Check bridge** – `FirebaseAppCheckInternal::token_provider()` exposes App Check credentials as a Firestore
  `TokenProvider`, making it straightforward to attach App Check headers when configuring the HTTP datastore.
- **Typed converters** – Collection and document references accept `with_converter(...)`, and typed snapshots expose
  converter-aware `data()` helpers that match the JS modular API.
- **Snapshot metadata** – `DocumentSnapshot` now carries `SnapshotMetadata`, exposing `from_cache` and
  `has_pending_writes` flags compatible with the JS API.
- **Query API scaffolding** – `Query`, `QuerySnapshot`, and `FirestoreClient::get_docs` now cover collection scans as
  well as `where` filters, `order_by`, `limit`/`limit_to_last`, projections, and cursor bounds when running against
  either the in-memory store or the HTTP datastore (which generates structured queries for Firestore’s REST API).
- **Sentinel transforms** – `FirestoreValue::server_timestamp`, `array_union`, `array_remove`, and `numeric_increment`
  mirror the modular SDK’s FieldValue sentinels, with server-side transforms wired through both datastores.
- **Advanced filters** – Array membership operators (`array-contains`, `array-contains-any`) and disjunctive filters
  (`in`, `not-in`) are validated client-side and encoded for both datastores, matching the JS SDK constraints.
- **Batched writes** – `WriteBatch` mirrors the modular SDK: set/update/delete operations queue up and commit atomically
  via the shared datastore pipeline, enabling multi-document mutations over HTTP or the in-memory store.

## Still to do

- Transactions, merge preconditions, and sentinel transforms aligned with the JS mutation queue.
- Field transforms such as server timestamps, array union/remove, and numeric increment.
- Real-time listeners, watch streams, and pending-write coordination for latency-compensation.
- Collection group queries, aggregation APIs, and bundle/named-query support.
- Offline persistence layers (memory + IndexedDB), LRU pruning, and multi-tab coordination.
- Bundle loading, named queries, and emulator-backed integration coverage.

This is enough to explore API ergonomics and stand up tests, but it lacks real network, persistence, query logic, and the
majority of the Firestore feature set.

## Next steps - Detailed completion plan

1. **Network layer**
   - Stand up a gRPC/WebChannel transport alongside the existing REST client so we can stream listen/write RPCs with
     shared auth/backoff logic.
   - Handle persistent listen/write stream lifecycles (resume tokens, exponential backoff, heartbeat) once gRPC is
     wired.
   - Add richer token refresh/emulator header support for long-lived streaming sessions.
2. **Snapshot & converter parity**
   - Flesh out `DocumentSnapshot`, `QuerySnapshot`, and user data converters to match the JS SDK, including `withConverter`
     support and typed accessors.
3. **Write operations**
   - Build client-side transactions (retries, preconditions, mutation queue) atop the shared commit + transform
     pipeline.
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
| P0 | `packages/firestore/src/remote/persistent_stream.ts`, `remote/datastore.ts` | New `src/firestore/remote/grpc/` plus updates to `remote/datastore/mod.rs` | Implement gRPC/WebChannel listen & write streaming alongside REST, sharing auth/backoff logic. | Requires platform stream adapters (native + wasm) and watch change parsing. |
| P0 | `packages/firestore/src/remote/datastore.ts` (credentials, emulator) | `src/firestore/remote/datastore/http.rs`, `mod.rs` | Expand token refresh/emulator header handling and unify transform-aware commits across transports. | Depends on auth/App Check providers, heartbeat storage, and gRPC transport availability. |
| P1 | `packages/firestore/src/api/transaction.ts`, `core/transaction_runner.ts` | `src/firestore/api/transaction.rs`, `src/firestore/core/transaction.rs` | Build transactions/retries and precondition support atop the new commit pipeline. | Needs gRPC commit parity and mutation queue abstractions. |
| P1 | `packages/firestore/src/api/snapshot.ts`, `api/reference_impl.ts` | `src/firestore/api/snapshot.rs`, `src/firestore/api/mod.rs` | Polish snapshot metadata/converters and align API surface with modular JS (`withConverter`, typed snapshots). | Requires gRPC listen results and converter parity. |
| P1 | `packages/firestore/src/api/filter.ts`, `core/query.ts`, `core/view.ts`, `remote/watch_change.ts` | `src/firestore/api/query.rs`, `src/firestore/core/query/` | Port query builders, bounds, ordering, and attach them to real listen results. | Requires streaming remote store, target serialization, and comparator logic. |
| P2 | `packages/firestore/src/local/indexeddb_*`, `local/persistence.ts`, `local/remote_document_cache.ts` | `src/firestore/local/` | Establish trait-based persistence with in-memory baseline, paving the way for disk-backed stores later. | Requires query engine integration and watch pipeline parity. |

Follow this order so that the network-backed datastore unblocks richer snapshots and transactional APIs before layering on
query/watch streaming and persistence. As each Rust module solidifies, port the matching TS unit tests from
`packages/firestore/test/unit/` to ensure behavioural parity.

Working through these steps in order will gradually move the port from a stubbed implementation to a fully compatible
Firestore client suitable for real workloads.
