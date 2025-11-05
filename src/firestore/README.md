# Firebase Firestore module

## Introduction

This module ports core pieces of the Firestore SDK to Rust so applications can
discover collections and create, update, retrieve, and delete documents. It
provides functionality to interact with Firestore, including retrieving and
querying documents, working with collections, and managing real-time updates.

It includes error handling, configuration options, and integration with
Firebase apps.

## Porting status

- firestore 83% `[######### ]`

==As of April 12th, 2026==

Roughly 83 % of the Firestore JS SDK now has a Rust counterpart.

- Core handles (component registration, `Firestore`/reference types, converters), document/query operations, and the HTTP datastore remain in place.
- Set semantics now mirror the JS SDK, including `SetOptions::merge`/`merge_fields`, allowing partial document updates through both the HTTP and in-memory datastores.
- All array/membership operators (`array-contains`, `array-contains-any`, `in`, `not-in`) are supported and validated; queries auto-normalise ordering just like the JS structured-query builder.
- Batched writes (`WriteBatch`) are fully wired, sharing the commit pipeline across HTTP and in-memory datastores so atomic groups of set/update/delete mirror `writeBatch()` from the modular SDK.
- Collection group queries now compile identical structured queries across the HTTP and in-memory datastores.
- Document snapshots expose `get(...)` helpers that align with the modular JS API for selective field reads.
- REST and in-memory datastores can execute aggregation queries (`count`, `sum`, `average`), matching `getAggregate()`/`getCount()` from the JS SDK.
- Remaining gaps focus on the sync engine: RemoteSyncer integration, transactions, offline persistence, cross-platform transports, and advanced bundle/listener plumbing.

## References to the Firebase JS SDK - firestore module

- QuickStart: <https://firebase.google.com/docs/firestore/quickstart>
- API: <https://firebase.google.com/docs/reference/js/firestore.md#firestore_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firestore>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/firestore>

## Development status as of 5th April 2026

- Core functionalities: Mostly implemented (see this README for details)
- Testing: 48 tests (covering merges, advanced filters, collection-group queries, aggregations, and existing update/delete flows)
- Documentation: Public APIs documented with rustdoc (set/merge semantics, `WriteBatch`, queries, snapshot accessors, aggregations)
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
if let Some(born) = alan_snapshot.get("born")? {
    if let ValueKind::Integer(year) = born.kind() {
        println!("Alan was born in {year}");
    }
}
let query = firestore.collection("users").unwrap().query();
let aggregates = client.get_count(&query).await?;
println!(
    "Total users: {}",
    aggregates.count("count")?.unwrap_or_default()
);
    Ok(())
}
```

If App Check is not enabled for your app, pass `None` as the third argument to
`with_http_datastore_authenticated`.

### Using Converters

```rust,ignore
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
  either the in-memory store or the HTTP datastore (which generates structured queries for Firestore's REST API).
- **Sentinel transforms** – `FirestoreValue::server_timestamp`, `array_union`, `array_remove`, and `numeric_increment`
  mirror the modular SDK's FieldValue sentinels, with server-side transforms wired through both datastores.
- **Advanced filters** – Array membership operators (`array-contains`, `array-contains-any`) and disjunctive filters
  (`in`, `not-in`) are validated client-side and encoded for both datastores, matching the JS SDK constraints.
- **Batched writes** – `WriteBatch` mirrors the modular SDK: set/update/delete operations queue up and commit atomically
  via the shared datastore pipeline, enabling multi-document mutations over HTTP or the in-memory store.
- **Collection-group queries** – `Firestore::collection_group` issues structured queries with `allDescendants`, and both
  datastores respect the same validation/ordering semantics as the JS SDK.
- **Snapshot field accessors** – `DocumentSnapshot::get` (and typed variants) accept `&str`/`FieldPath` inputs, returning
  borrowed `FirestoreValue`s for selective field reads just like `DocumentSnapshot.get(...)` in the modular API.
- **Aggregations** – `FirestoreClient::get_aggregate`/`get_count` surface REST `runAggregationQuery`, while the
  in-memory datastore computes `count`, `sum`, and `average` locally for tests.
- **Streaming scaffolding** – A multiplexed stream manager provides shared framing over arbitrary transports, with
  in-memory and WebSocket transports plus tests validating multi-stream exchange and cooperative shutdown behaviour.
- **Streaming datastore** – Exposed a generic `StreamingDatastore` over the multiplexed streams so listen/write state
  machines can share connection management with concrete transports.
- **Persistent stream runner** – Added a reusable state machine that re-opens listen/write streams with exponential
  backoff and delegate callbacks, paving the way for the Firestore watch/write RPC loops.
- **Network layer** – `firestore::remote::network::NetworkLayer` now coordinates credential-aware persistent listen
  and write streams over the multiplexed transport, reusing backoff logic and compiling for both native and WASM
  targets.
- **Listen/write streaming** – `firestore::remote::streams::{ListenStream, WriteStream}` encode Firestore gRPC listen
  and write RPC payloads, propagate resume tokens, and surface decoded `WatchChange`/`WriteResponse` events through async
  delegates. `firestore::remote::watch_change_aggregator::WatchChangeAggregator` converts those into consolidated
  `RemoteEvent`s so higher layers can drive query views.
- **Remote store façade** – `firestore::remote::remote_store::RemoteStore` now owns listen/write stream lifecycles,
  feeds watch events into `RemoteSyncer` implementors, keeps per-target metadata via `TargetMetadataProvider`, and
  drains mutation batches over the write stream with wasm-friendly future aliases.
- **Remote syncer bridge** – `firestore::remote::syncer_bridge::RemoteSyncerBridge` brings the JS `RemoteSyncer`
  surface to Rust, tracking remote keys per target, managing the mutation batch queue, and delegating watch/write events
  through wasm-friendly futures so higher layers (local store, query views) can plug in incrementally.
- **Memory local store** – `firestore::local::MemoryLocalStore` implements the new `RemoteSyncerDelegate`, letting tests
  and prototype sync engines observe document state, pending batches, stream metadata, overlays, and limbo resolutions
  without depending on the future persistence layer. It now mirrors the JS LocalStore's target bookkeeping (remote keys,
  resume tokens, snapshot versions) while coordinating with the remote store for both listen and write pipelines across
  native and wasm targets. A WASM build can persist the snapshots via IndexedDB through the bundled
  `IndexedDbPersistence` adapter.
- **Sync engine scaffold** – `firestore::local::SyncEngine` wires `MemoryLocalStore` and `RemoteStore` together, seeding
  restored target metadata into the remote bridge and exposing a façade that mirrors the JS SyncEngine API for
  registering listens, pumping writes, and toggling network state.
- **Query listeners** – `SyncEngine::listen_query` now hooks query/view updates into the sync engine. Registered
  listeners receive live `QuerySnapshot`s when remote events or overlay changes land in `MemoryLocalStore`, and
  `QueryListenerRegistration` handles make it easy to stop listening.
- **Query view integration** – Listener snapshots now evaluate filters, ordering, bounds, and limits locally, surface
  ViewSnapshot-style metadata (`from_cache`, `has_pending_writes`, `sync_state_changed`), expose per-target resume tokens
  so consumers can persist listen state across disconnects, apply pending write overlays so latency-compensated data
  matches local user edits, and emit JS-style doc change sets (`added`/`modified`/`removed`) with positional indices.

## Still to do

- Transactions and merge preconditions aligned with the JS mutation queue.
- Hook the new remote store into the higher-level sync engine/local store once those layers are ported, including limbo
  resolution, existence-filter mismatch recovery, and overlay diff reconciliation across targets.
- Offline persistence layers (memory + IndexedDB), LRU pruning, and multi-tab coordination.
- Bundle loading, named queries, and enhanced aggregation coverage (min/max, percentile).
- Emulator-backed integration coverage and stress tests across HTTP/gRPC transports.

This is enough to explore API ergonomics and stand up tests, but it lacks real network, persistence, query logic, and the
majority of the Firestore feature set.

## Next steps - Detailed completion plan

1. **View diffing & overlay reconciliation**

   - Compute document change sets (`added`/`modified`/`removed`) plus mutated-key tracking so higher layers can surface
     JS-style `docChanges` without re-walking entire snapshots.
   - Apply pending write overlays when constructing view snapshots (including delete overlays) so latency-compensated
     writes update query results immediately with the expected field values.
   - Track limbo documents and existence-filter mismatches, triggering target resets and recovery listens when watch
     responses signal inconsistent remote state.
2. **Snapshot & converter parity**
   - Flesh out `DocumentSnapshot`, `QuerySnapshot`, and user data converters to cover remaining lossy conversions (e.g.,
     snapshot options, server timestamps) and ensure typed snapshots expose all JS helpers.
3. **Write operations**
   - Build client-side transactions (retries, preconditions, mutation queue) atop the shared commit + transform
     pipeline.
4. **Query engine**
   - Finish porting the remaining query builder surface (composite/OR filters, `orderBy` on nested/transform fields,
     cursor helpers such as `startAfter`/`endBefore`, limit-to-last validation) so it mirrors `packages/firestore/src/core/query.ts`.
   - Implement target serialization and comparator logic shared by the local store and the remote watch layer so
     listen responses can be applied to views.
   - Connect the normalised query definitions to the remote listen stream once gRPC/WebChannel support lands, ensuring
     resume tokens, ordering, and backfill handling behave identically to the JS SDK.
5. **Local persistence**
   - Introduce the local cache layers (memory, IndexedDB-like, LRU pruning) and multi-tab coordination, matching the JS
     architecture.
6. **Bundles & advanced aggregates**
   - Port bundle loading, named queries, and extended aggregate functions (min/max, percentile) once core querying is
     functional.
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
