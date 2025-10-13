## 12 October 2025

Firestore Port Plan

  - Inventory & Mapping: Catalogue packages/firestore entry points (public API surface, component registration,
  platforms) and map each TS module to a Rust target file/module. Note shared dependencies (auth/app/util) to confirm
  existing Rust ports cover them.
  - Define Rust Crate Structure: Sketch the desired src/firestore/ module tree (e.g. api.rs, service.rs, query.rs,
  document.rs, serializer.rs, platform backends). Decide on common utilities that should live in shared crates (util,
  networking, serialization).
  - Translate Core Types: Port public-facing types (Firestore, DocumentReference, Query, CollectionReference, snapshots)
  and value primitives (FieldValue, GeoPoint, Timestamp). Mirror TS interfaces with idiomatic Rust structs/enums.
  - Component Wiring: Register firestore and firestore-internal components against the app container, mirroring the
  JS ComponentType usage. Ensure dependency injection hooks (for auth/app-check) align with existing Rust component
  framework.
  - Networking/Proto Layer: Port the REST/gRPC adapter (api/database fetch logic, serializers) including request queue,
  backoff, and persistence hooks. Decide whether to stub or implement the full persistence layer initially.
  - Operational Logic: Implement querying, document reads/writes, batch/transaction scaffolding—possibly staged (start
  with simple reads/writes before offline/persistence features).
  - Error & Logging Parity: Port error codes/enums, logging categories, and debug instrumentation used across the
  module.
  - Testing Strategy: Replicate unit tests from TS (value conversions, comparator ordering, query building). Add
  integration-style tests once network stubs exist (potentially mocking backends).
  - Documentation & Examples: Add README usage snippets and update crate-level docs to explain the Rust API.
  - Follow-up: After the minimal working surface, iterate on advanced features (bundles, cache, multi-tab persistence)
  once foundational networking and serialization are stable.


Let's start with the first two steps

• Explored
  └ List firestore
    Search find packages/firestore/src -maxdepth 2 -type f
    List firestore
    Read package.json

> 1. Firestore JS Surface Inventory & Mapping

  - Entry points
      - src/index.ts, index.node.ts, index.rn.ts, plus register.ts wire component registration, version logging, and
  platform-specific exports.
      - src/api/*.ts holds public modular APIs (database bootstrap, refs, queries, transactions, bundles, aggregation,
  field utilities).
      - src/lite-api re-exports a reduced surface that bypasses persistence/offline bits.
  - Core subsystems
      - auth/credentials.ts: credential providers, token helpers.
      - core/*: main client, transactions, query engine, sync engine.
      - local/*: local persistence layers (LRU, multi-tab, bundle cache).
      - remote/*: network stack (datastore, streams, watch/listen, write pipeline) built on gRPC/webchannel.
      - model/*: document/value representations (Document, Mutation, FieldValue, Path, Timestamp, GeoPoint).
      - util/*: shared helpers (async queue, byte strings, debug asserts, error codes).
      - platform/*: browser/node specific glue (component registration, persistence toggles).
      - protos/*: generated typings for Firestore gRPC protos.
  - Tests
      - test/unit/... mirrors each subsystem; lite tests for reduced API; emulator test runners in scripts/.

  High-Level Mapping Idea

  | JS Area | Proposed Rust Target |
  |---------|---------------------|
  | src/index*.ts, register.ts | src/firestore/mod.rs, component.rs, version.rs |
  | api/ | src/firestore/api/ (e.g., database.rs, reference.rs, query.rs, field_value.rs, bundle.rs) |
  | auth/ | Reuse existing auth/app_check providers; add Firestore-specific credential adapters in src/firestore/auth.rs
  |
  | core/ | src/firestore/core/ (firestore_client.rs, transaction.rs, sync_engine.rs) |
  | local/ | src/firestore/local/ (persistence traits, SQLite/IndexedDB stubs) |
  | remote/ | src/firestore/remote/ (datastore, streams, serializer) |
  | model/ | src/firestore/model/ (document.rs, mutation.rs, value.rs, path.rs) |
  | util/ | Extend shared src/util/ when applicable; Firestore-specific helpers live in src/firestore/util.rs |
  | platform/ | src/firestore/platform/ for runtime-specific toggles |
  | protos/ | Replace with Rust protobuf/tonic definitions later (possibly in src/firestore/protos/ or separate crate) |
  | lite-api/ | src/firestore/lite/ re-exporting minimal API |

  2. Proposed Rust Module Skeleton

  src/firestore/
    mod.rs             // top-level exports, component registration
    component.rs       // storage-type constants, provider hookup
    version.rs         // register_version utilities

    api/
      mod.rs
      database.rs      // `get_firestore`, `initializeFirestore`
      reference.rs     // DocumentReference, CollectionReference
      query.rs         // Query, QueryConstraint, orderBy/where builders
      transaction.rs   // Transaction, WriteBatch entry points
      field_value.rs   // FieldValue, FieldPath helpers
      bundle.rs        // LoadBundle API scaffolding
      aggregate.rs     // count/sum/avg wrappers
      settings.rs      // FirestoreSettings, cache config

    auth.rs            // credentials and emulator token wiring

    model/
      mod.rs
      value.rs         // FirestoreValue enum, conversions
      path.rs          // ResourcePath, FieldPath
      document.rs
      mutation.rs
      timestamp.rs
      geopoint.rs
      database_id.rs

    core/
      mod.rs
      firestore_client.rs
      sync_engine.rs
      transaction.rs
      event_manager.rs

    local/
      mod.rs
      persistence.rs   // trait definitions
      memory_persistence.rs (initial stub)
      bundle_cache.rs
      query_cache.rs

    remote/
      mod.rs
      datastore.rs
      stream.rs        // watch/write streams
      serializer.rs    // JSON/Protobuf mapping

    util.rs            // Firestore-specific helpers (AsyncQueue equivalent, ExponentialBackoff reuse?)

    platform/
      mod.rs
      browser.rs       // persistence availability flags
      node.rs          // node-specific credential handling

    lite/
      mod.rs           // re-export subset of api

    protos/
      mod.rs           // placeholder for generated protobuf structs (future)

  - Shared types (timestamps, ByteString) may live in src/util/ if used broadly.
  - Networking & persistence layers can start as stubs returning NotImplemented errors until fully ported.
  - Component registration should mirror existing storage module pattern, adding both public (firestore) and internal
  (firestore-internal) providers once defined.

  Next steps beyond this foundation: prioritize porting the model layer and minimal API (get_firestore,
  DocumentReference path handling) before tackling the heavy core/local/remote logic.






I’m putting together a final message that highlights adding the Firestore module skeleton and re-exports, implementing
  key model types like ResourcePath and DatabaseId with file references, as well as the minimal API with Firestore
  struct and registration. I’ll mention tests for unique app names and storage test tweaks, referencing relevant files
  and lines. I’ll also note calling register_version and register_firestore_component and the use of the unique_settings
  function to avoid duplicates, finishing by pointing out that cargo test and cargo check ran successfully.

> Implemented a foundational Firestore port with minimal API support and refreshed tests.

  - Introduced the Firestore module skeleton with public re-exports and core constants/error wiring so callers can use
  crate::firestore::* (src/firestore/mod.rs:1, src/firestore/constants.rs:1, src/firestore/error.rs:1).
  - Ported key model primitives—ResourcePath, FieldPath, DatabaseId, DocumentKey, Timestamp, and GeoPoint—to mirror the
  JS path/value semantics and added unit tests for each (src/firestore/model/resource_path.rs:1, src/firestore/model/
  field_path.rs:1, src/firestore/model/database_id.rs:1, src/firestore/model/document_key.rs:1, src/firestore/model/
  timestamp.rs:1, src/firestore/model/geo_point.rs:1).
  - Implemented a minimal Firestore API layer: component registration, get_firestore, and typed CollectionReference/
  DocumentReference helpers with auto-ID support and validation (src/firestore/api/database.rs:1, src/firestore/api/
  reference.rs:1). Added pub mod firestore; to the crate root so the surface is available to consumers (src/lib.rs:6).
  - Updated storage tests to give each temporary app a unique name, preventing DuplicateApp collisions now that more
  integration tests run (src/storage/reference.rs:68).

  cargo fmt and cargo test both pass.




I’m putting together a final message that highlights adding the Firestore module skeleton and re-exports, implementing
  key model types like ResourcePath and DatabaseId with file references, as well as the minimal API with Firestore
  struct and registration. I’ll mention tests for unique app names and storage test tweaks, referencing relevant files
  and lines. I’ll also note calling register_version and register_firestore_component and the use of the unique_settings
  function to avoid duplicates, finishing by pointing out that cargo test and cargo check ran successfully.

> Implemented a foundational Firestore port with minimal API support and refreshed tests.

  - Introduced the Firestore module skeleton with public re-exports and core constants/error wiring so callers can use
  crate::firestore::* (src/firestore/mod.rs:1, src/firestore/constants.rs:1, src/firestore/error.rs:1).
  - Ported key model primitives—ResourcePath, FieldPath, DatabaseId, DocumentKey, Timestamp, and GeoPoint—to mirror the
  JS path/value semantics and added unit tests for each (src/firestore/model/resource_path.rs:1, src/firestore/model/
  field_path.rs:1, src/firestore/model/database_id.rs:1, src/firestore/model/document_key.rs:1, src/firestore/model/
  timestamp.rs:1, src/firestore/model/geo_point.rs:1).
  - Implemented a minimal Firestore API layer: component registration, get_firestore, and typed CollectionReference/
  DocumentReference helpers with auto-ID support and validation (src/firestore/api/database.rs:1, src/firestore/api/
  reference.rs:1). Added pub mod firestore; to the crate root so the surface is available to consumers (src/lib.rs:6).
  - Updated storage tests to give each temporary app a unique name, preventing DuplicateApp collisions now that more
  integration tests run (src/storage/reference.rs:68).

  cargo fmt and cargo test both pass.


Updated Plan
  └ ✔ Port Firestore Value representations (scalar, map, array) and Base64 byte helper with tests.
    ✔ Implement minimal document snapshot and write payload structures leveraging Value types.
    ✔ Add stubbed remote datastore client using reqwest for basic get/set operations.
    ✔ Expose modular API helpers (`get_doc`, `set_doc`, `add_doc`) using datastore stub; update component wiring/tests.
    ✔ Format and run tests to validate.

• Ran git status -sb

> Extended the Firestore port with a usable value layer and a stubbed document API.

  - Added Firestore value primitives (ArrayValue, MapValue, BytesValue, and FirestoreValue) mirroring the JS field
  encodings, including unit coverage (src/firestore/value/*).
  - Introduced minimal document operations: snapshots, set options, validation helpers, and an in-memory Datastore
  that backs a new FirestoreClient with get_doc, set_doc, and add_doc support (src/firestore/api/operations.rs, src/
  firestore/remote/datastore.rs, src/firestore/api/document.rs).
  - Wired the new API surface into firestore::api exports so callers can use FirestoreClient alongside the existing
  references (src/firestore/api/mod.rs, src/firestore/mod.rs).
  - Broadened tests to cover the new value and document paths; all suites continue to pass (cargo test).

  Next steps: 1) expand FirestoreClient beyond the in-memory stub—start porting the real network datastore with request
  serialization; 2) flesh out document snapshots with typed data extraction and integrate with reference API ergonomics.



## 12 October 2025 - second run

I'll wrap up with a final message that references the exact lines added in the README, notes that tests weren't
  run since it's a doc-only change, and offers some logical next steps like verifying dependencies or starting
  implementation tasks. This should keep everything clear and organized!

> Added an actionable roadmap that maps concrete JS sources to their Rust destinations so we can sequence the remaining
  Firestore porting work: see the new “Immediate Porting Focus” table in src/firestore/README.md:47-59. It prioritises
  replacing the stubbed datastore, expanding snapshots/converters, wiring write batches/transactions, and standing up
  the query builder before tackling persistence.

  Tests: not run (doc-only change).

  Next steps: 1) Tackle the P0 datastore row—design the Rust networking layer and identify any shared HTTP/auth
  primitives we need to extract.



  Thinking
  I'll prepare a clear overview highlighting the new HTTP connection and error mapping with precise file and line references, describe the datastore trait restructuring with token provider support,
  document the updated FirestoreClient with new builder methods, mention README improvements, and note the re-export of DocumentSnapshot. I'll also cover test execution results, address the TODO on
  restful set_doc merge support, and suggest next steps focused on real token integration and merge implementation while briefly noting untouched pre-existing log file changes.

• Explored
  └ Read README.md

> Thinking
  I’m outlining a concise summary that highlights key updates: the new HTTP remote stack (connection builder, serializer, error translator, retrying datastore), standardized datastore traits with
  token providers and in-memory impl, FirestoreClient improvements with re-exports and new error codes, plus README updates. I’ll note the set_document merge support is unimplemented and token
  providers default to noop. Tests were run, and next steps include wiring real token providers and implementing merge and watchers.

> Overview

  - Built the network foundation for Firestore: a reusable HTTP connection with emulator awareness (src/firestore/remote/connection.rs:15), Firestore JSON serializer/deserializer (src/firestore/
  remote/serializer.rs:13), RPC->Firestore error mapping (src/firestore/remote/rpc_error.rs:22), and a retrying HttpDatastore that pulls auth/app-check tokens via pluggable providers (src/firestore/
  remote/datastore/http.rs:18).
  - Refactored the datastore layer into a trait plus token-provider abstraction so both the new HTTP client and the existing in-memory store plug in cleanly (src/firestore/remote/datastore/mod.rs:11,
  src/firestore/remote/datastore/in_memory.rs:13); exposed the remote module publicly for consumers (src/firestore/mod.rs:4).
  - Expanded the Firestore error surface so HTTP status translations land on meaningful SDK codes (src/firestore/error.rs:6) and re-exported DocumentSnapshot/SetOptions for crate-wide use (src/
  firestore/api/mod.rs:5); FirestoreClient now accepts any datastore, with helpers for in-memory and HTTP backends (src/firestore/api/document.rs:13,28).
  - Updated Firestore’s README with an immediate porting roadmap plus the revised client constructor using the convenience in-memory backend (src/firestore/README.md:47,82).
  - Note: repository already contained unrelated LOG/JSEXAMPLE changes that remain untouched.

  Tests

  - cargo test

  Next Steps

  1. Wire real auth/app-check token providers into HttpDatastore::builder, and decide how to surface emulator/timeout overrides to callers.
  2. Implement merge/updateMask handling and the remaining write/query RPCs so the HTTP datastore reaches feature parity with the JS client.


   so you are suggesting that now we improve auth and app-check?

> Not necessarily—those next steps were just suggestions if you plan to keep pushing on the networked datastore soon. If you’d rather tackle something else (queries, snapshots, etc.), feel free.
  The current wiring already uses no-op token providers, so it’s safe to leave auth/app-check integration for later unless you specifically need authenticated calls. Let me know what you’d like to
  prioritize next!



  Thinking
  I see many files with changes beyond what I worked on, likely pre-existing. I want to confirm I didn’t alter those
  unintentionally and ensure the final summary notes that other local modifications remain untouched. I’ll also update
  the plan to mark steps 2 and 3 as completed to keep things organized.

• Updated Plan
  └ ✔ Implement App Check token provider that conforms to Firestore TokenProvider (incl. tests).
    ✔ Wire providers into Firestore HTTP datastore and retry handling, adjust APIs and docs.
    ✔ Format, run tests, and update README with new guidance.

• Explored
  └ Read token_provider.rs, interop.rs, mod.rs, document.rs, http.rs, README.md


Explored
  └ Read token_provider.rs, interop.rs, mod.rs, document.rs, http.rs, README.md

> Thinking
  I’m preparing a detailed final message that highlights the new App Check token provider integration, including the
  new token_provider method on FirebaseAppCheckInternal and updates in FirestoreClient and RetrySettings. I’ll mention
  the new file src/app_check/token_provider.rs and reference relevant line numbers. I’ll also note the tests we ran with
  cargo test that failed due to sandbox restrictions blocking httpmock from binding to localhost, explaining the context
  of these failures. Additionally, I’ll acknowledge pre-existing unrelated repo modifications and mention we ran cargo
  fmt. The message will conclude with suggested next steps like porting emulator header logic, structured clearly and
  concisely as requested.

> Added an App Check credential bridge so Firestore’s HTTP client can use real tokens and recover from auth errors. New
  AppCheckTokenProvider implements the Firestore TokenProvider contract, includes helper app_check_token_provider_arc,
  and comes with unit coverage for happy-path and error mapping (src/app_check/token_provider.rs:10-153).
  FirebaseAppCheckInternal now exposes a ready-made token provider hook (src/app_check/interop.rs:61-64), allowing
  consumers to hand the same provider to Firestore.

  FirestoreClient::with_http_datastore_authenticated accepts an optional App Check provider and forwards it through the
  builder (src/firestore/api/document.rs:33-46). Retry logic now treats Unauthenticated as transient and has regression
  tests to prove it (src/firestore/remote/datastore/http.rs:207-257). The Firestore README reflects the new wiring and
  shows how to pass App Check credentials (src/firestore/README.md:17-123).

  Formatting: cargo fmt

  Tests: cargo test (fails in sandbox – local Hyper/httpmock servers cannot bind to 127.0.0.1 under seccomp)

  Natural follow-up once tokens land end-to-end: add emulator header handling and richer refresh heuristics for both
  credential providers.

  Thinking
  I’m preparing a final message that explains the new snapshot module with metadata, updates to datastores, and
  operations changes including file references with specific line mentions. I'll highlight changes to DocumentSnapshot
  constructors and that tests ran but failed due to sandbox restrictions blocking network calls, especially for
  httpmock. I’ll also note the README updates and caution about the default metadata hookup still being false. Next,
  I’ll suggest finishing the work by linking metadata to real data sources.

> Added a dedicated snapshot module so DocumentSnapshot now carries SnapshotMetadata and exposes from_cache/
  has_pending_writes flags consistent with the JS API (src/firestore/api/snapshot.rs:12). Updated both datastore
  backends to construct snapshots with this metadata scaffold (src/firestore/remote/datastore/in_memory.rs:4, src/
  firestore/remote/datastore/http.rs:7) and reworked the API exports/tests to pull in the new definitions (src/
  firestore/api/mod.rs:1, src/firestore/api/operations.rs:1). The Firestore README now documents the snapshot metadata
  milestone (src/firestore/README.md:9).

  Tests: cargo fmt; cargo test (fails in sandbox because hyper/httpmock cannot bind to 127.0.0.1 under seccomp).

  Next steps you may want to consider: 1) Plumb real cache/pending-write flags once the remote datastore surfaces
  metadata; 2) Extend the new snapshot module with converter-aware accessors to mirror JS withConverter ergonomics.

  what do you mean with converter-aware accessors?

> In the JS SDK, if you call .withConverter(myConverter) on a DocumentReference or Query, the resulting snapshots expose
  extra helpers like snapshot.data() and snapshot.get() that automatically pass the raw Firestore data through the
  user‑supplied converter. Right now our Rust DocumentSnapshot only exposes the raw BTreeMap<String, FirestoreValue>.

  By “converter‑aware accessors” I meant adding Rust equivalents—methods (or trait hooks) that, when a converter has
  been attached to the reference/query, return your strongly typed model instead of the raw map. That would mirror the
  modular JS API and let callers opt into typed serialization while keeping the metadata/path handling we’ve already
  implemented.

▌ ok, let's implement both, the converter-aware accssors and the cache/pending-write

• Updated Plan
  └ □ Design snapshot converter hooks and metadata source; update snapshot struct to store converter-aware state.
    □ Plumb metadata flags from datastores and add converter-aware APIs/tests.
    □ Integrate with Firestore client factories, update docs/readme, run fmt/tests.

• Explored
  └ Search with_converter

  Analyzing snapshot metadata and converters (18s • Esc to interrupt)


  Thinking
  I'll put together a clear summary outlining key code changes, file references with line numbers, test results
  including failures due to HTTP mock sandbox issues, and suggested next steps. This will be neatly formatted with
  bullets to make it easy to scan and understand.

> - Added a reusable converter abstraction so callers can translate between domain models and Firestore maps via
  FirestoreDataConverter and the built-in PassthroughConverter (src/firestore/api/converter.rs:6-32).
  - Reworked document references and snapshots to carry converter context and metadata: with_converter now yields
  typed references, TypedDocumentSnapshot exposes converter-aware data(), and snapshots surface real from_cache/
  has_pending_writes flags (src/firestore/api/reference.rs:62-227, src/firestore/api/snapshot.rs:12-166).
  - Extended FirestoreClient with typed read/write helpers and regression tests that validate converter round-trips
  while asserting metadata coming from the in-memory datastore (src/firestore/api/document.rs:3-211).
  - Wired metadata defaults through both datastores and documented the new converter flow in the Firestore README for
  easy discovery (src/firestore/remote/datastore/in_memory.rs:1-35, src/firestore/remote/datastore/http.rs:7-145, src/
  firestore/README.md:7-159).

  Tests:

  - cargo fmt
  - cargo test (fails under sandbox because httpmock cannot bind to 127.0.0.1; all Firestore-specific tests passed
  before the network-bound fixtures ran)


  I'll outline steps to systematically port tests from the JS codebase using mocha to Rust's cargo test framework,
  starting with an inventory and mapping of existing tests to Rust modules, then addressing dependencies and mocks, and
  possibly creating bridging test harnesses for module parity. I'll also consider which utilities may need rewriting and
  prioritize unit tests for already ported modules while keeping integration tests and emulator bridging on the roadmap.

• Explored
  └ List unit

> Thinking
  I’m putting together a detailed plan to port Firestore JavaScript tests to Rust, focusing first on unit tests and
  mapping JS test modules to Rust equivalents. The plan includes phases for prioritizing high-value tests, building
  a Rust test support crate, replicating JS test helpers, and eventually handling integration tests with emulator
  dependencies. I'll incorporate references to existing test utilities, propose creating test directories under tests/
  firestore, and suggest using features like rstest to organize testing while acknowledging current network limitations.

• Explored
  └ Read helpers.ts

> Thinking
  I’m organizing a thorough plan to port tests by categorizing directories, mapping to Rust modules, and defining
  phased efforts from unit to integration tests aligned with current module porting. I’ll include setup of a shared
  test_support harness, handling test utilities, and propose a directory structure in Rust, with clear steps and gating
  for priorities like watchers and emulator use. This plan will help guide a systematic migration with well-defined
  tasks and timelines.

> Test Suite Overview

  - packages/firestore/test/unit/** – fine‑grained specs for model primitives, serializer logic, local persistence,
  query engine pieces, API surface, and various utilities. Heavy reuse of helpers in test/util/helpers.ts, test/unit/
  util/*.ts, and generated spec JSON.
  - packages/firestore/test/lite/** – parity checks for the lite entry points (subset of API coverage, overlaps with
  unit lite-api tests).
  - packages/firestore/test/integration/** – emulator-driven end‑to‑end tests covering network, multi-client
  coordination, persistence, and bundles. Depend on async queue, RPC stubs, and Firestore emulator instances.
  - Shared infra: test/util (document builders, fake serializers, comparator helpers), test/unit/bootstrap.ts
  (configures mocha/chai, fake timers), and scripts that synthesize JSON spec cases used by watch/write pipelines.

  Porting Plan

  1. Map Unit Suites to Rust Modules
      - Inventory each test/unit subdirectory and match to current Rust code (model, value, remote::serializer,
  api::reference, etc.).
      - Document gaps (e.g., JS core/view tests need Rust equivalents once that layer exists) so we can time the port
  with feature availability.
  2. Establish a Rust Test Harness
      - Under src/firestore/test_support create utilities mirroring test/util/helpers.ts (builders for docs, versions,
  paths, comparators).
      - Provide reusable fixtures for MutableDocument, SnapshotVersion, fake JsonProtoSerializer, etc., so unit tests
  stay concise.
  3. Phase 1: Model & Value Parity
      - Translate test/unit/model/*.test.ts and test/unit/util/*.ts cases into Rust module tests (some already exist but
  need expanded coverage).
      - Ensure helper equivalence (e.g., wrapObject, byte normalization, mutation encoders).
  4. Phase 2: API / Reference / Snapshot Tests
      - Port test/unit/api & test/unit/lite-api scenarios that exercise DocumentReference, CollectionReference,
  withConverter, and snapshot metadata.
      - Use the new converter-aware Rust APIs with the in-memory datastore; add typed fixture converters to mirror JS
  behavior.
  5. Phase 3: Remote Serializer & Network Edge
      - Adapt test/unit/remote/* that validate HTTP/stream serializer logic, existence filters, watch changes.
      - Where network features are missing, gate the tests behind #[cfg(feature = "wasm-web")] or mark TODOs to unblock
  once the corresponding Rust modules land.
  6. Phase 4: Local Persistence & Core Engine
      - When Rust gains local store/query engine pieces, port test/unit/local/* and test/unit/core/*, reusing the JS
  spec JSON if practical (consider storing the generated JSON fixtures under tests/data/firestore).
      - Build Rust equivalents for async queue/fake scheduler utilities to mirror deterministic execution.
  7. Plan for Integration Coverage
      - Defer test/integration/** until Rust has emulator/network features; outline future work to run against Firebase
  emulator (likely behind an optional cargo feature with environment configuration).
      - Capture dependencies (Auth/AppCheck token providers, RPC mocks) so the groundwork is ready.
  8. Testing Infrastructure
      - Organize Rust tests into tests/firestore/unit/... plus inline module tests where appropriate.
      - Standardize assertion helpers and golden comparisons (e.g., using insta snapshots where JS tests rely on
  stringified proto output).
  9. Tracking & Reporting
      - Maintain a checklist mapping each JS test file to its Rust counterpart.
      - Update src/firestore/README.md “Next Steps” with testing milestones and link the plan so progress is visible.

  This sequence lets us bring over deterministic unit coverage first, using the in-memory datastore and serializer
  scaffolding already in place, while deferring emulator-bound scenarios until the networking surface is ready.


  Ran git status -sb

> - Added reusable helpers under src/firestore/test_support/path.rs so future tests can build ResourcePath/FieldPath
  instances succinctly.
  - Expanded ResourcePath/DocumentKey APIs to match JS ergonomics (get, pop_first_n, without_last, is_prefix_of,
  comparator) and added small conveniences for FieldPath.
  - Ported the packages/firestore/test/unit/model/path.test.ts scenarios into Rust integration tests (tests/firestore/
  model/resource_path_tests.rs), covering construction, traversal, prefix checks, and comparator ordering.

  Tests:

  - cargo fmt
  - cargo test resource_path_tests (fails to run under sandbox because integration binaries are blocked; unit build
  succeeded)

  dded comprehensive Rust docs for the Firestore surface so users can discover APIs without cross-referencing the TS
  sources:

  - Documented the Firestore handle and registration helpers, covering collection/document lookup semantics (src/
  firestore/api/database.rs:38-153).
  - Described FirestoreClient constructors and data ops, including converter-aware helpers, mirroring the JS behavior
  (src/firestore/api/document.rs:22-147).
  - Clarified how collection/document references, typed references, and converters behave (src/firestore/api/
  reference.rs:31-225), and explained the converter trait itself (src/firestore/api/converter.rs:6-35).
  - Added snapshot metadata/context docs so callers know how to interpret cache/pending-write flags and convert data
  (src/firestore/api/snapshot.rs:12-167).
  - Documented SetOptions to highlight merge semantics (src/firestore/api/operations.rs:7-12).

  cargo check confirms the updates build cleanly.