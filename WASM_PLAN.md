# Async + WASM Migration Plan

This plan captures the work required to ship the next major version of the firebase-rs-sdk with async-first APIs and full support for the `wasm32-unknown-unknown` target. Executing in dependency order keeps the tree buildable: we start with the `app` container, then migrate the identity backbone (`auth`, `app_check`, `messaging`), and finally sweep the remaining feature modules. Throughout the effort we favour clean async APIs that mirror Firebase JS naming while running in both native and WASM environments.

## Focus Areas
1. Land Stage 1 by making the `src/app` module fully async-aware and wasm-ready, unblocking downstream crates and documenting any temporary gating.
2. Deliver Stage 2 by porting `auth`, `app_check`, and `messaging` to the shared async platform so every consumer can rely on the same token and messaging contracts. **Status:** ✅ Completed – all three modules now expose async APIs, share the token provider abstraction, and schedule timer-driven refresh using the new platform runtime.
3. Execute Stage 3 across the remaining modules, completing the Storage/Database async adoption, updating examples, and tightening wasm feature guards. Current priority order (most urgent first): **Firestore → Storage → Installations → Remote Config → other modules.**
4. This is a major version update. It is OK to make disrupting changes to the old API and change the public API to async when needed/recommended. Creating a clean, wasm compatible library is more important than legacy to the old API. Do not waste time trying to retain the old API. When a module blocks progress, comment out the offending imports with a `// TODO(async-wasm): re-enable once <reason>` note so the workspace keeps moving.

## Stage 0 – Tooling & Policy Prerequisites
- [x] Ensure the repository builds for `wasm32-unknown-unknown` using `cargo check --target wasm32-unknown-unknown --features wasm-web`.
  - 2025-02-14: Verified `cargo check --target wasm32-unknown-unknown --features wasm-web` (with and without `experimental-indexed-db`) succeeds after re-enabling the `app_check` module for wasm.
  - [✓] Adding the `wasm32-unknown-unknown` target locally (`rustup target add wasm32-unknown-unknown`).
  - [x] Audit workspace dependencies (e.g. `getrandom`, `reqwest`, `openssl`) for wasm support and enable or replace features as needed.
    - 2025-02-14: Confirmed current core deps (`reqwest`, `rand`/`getrandom`, `chrono`, `gloo-timers`) compile on `wasm32-unknown-unknown` with the existing cfg-gated feature flags. Remaining blocking-only modules (database, functions, storage, etc.) stay disabled behind `// TODO(async-wasm)` stubs until their async ports land.
  - [x] Document the required feature flags/commands and add a quickstart section to the contributor docs.
    - 2025-02-14: Added a "WASM build and test quickstart" section in `CONTRIBUTING.md` covering the `wasm-web` feature flag, target installation, and the `cargo check`/`cargo test wasm_smoke` commands.
- [x] Publish an async/WASM migration checklist covering naming parity (no `_async` suffixes), feature gating, executor expectations, and the temporary-disable/TODO pattern.
  - 2025-02-14: Added `docs/async_wasm_checklist.md` summarising the conventions; future porting work should reference this document.
- [x] Provide a local smoke-test script (or `just` recipe) that runs native linting plus `cargo test --target wasm32-unknown-unknown --features wasm-web` so contributors validate both targets consistently.
  - 2025-02-14: Added `scripts/smoke.sh` and `scripts/smoke.bat` covering `cargo fmt`, a trimmed native test run (skipping network-bound cases), `cargo check --target wasm32-unknown-unknown --features wasm-web`, and the wasm smoke test. The scripts automatically warn and skip the wasm runner when `wasm-bindgen-test-runner` is not installed.
- [x] Review shared crates under `src/component`, `src/util`, `src/logger`, and `src/platform` and note any blockers that must be handled before Stage 1 proceeds.
  - 2025-02-14: Confirmed the shared crates use async-friendly primitives only; no `std::thread`/blocking IO remains. `platform::runtime` already switches between Tokio and `wasm-bindgen-futures`, and `platform::browser::indexed_db` cleanly stubs when the experimental feature is disabled. No additional blockers identified for wasm.

## Stage 1 – App Module Foundation (`src/app`)
- [x] Refactor the `app` module so its public APIs return futures where appropriate while preserving Firebase JS naming.
- [x] Introduce async-aware initialisation logic that can run in wasm (`wasm-bindgen-futures`) and native (`tokio` optional feature) environments.
- [x] Audit module registration/component bootstrap paths to remove blocking primitives; migrate them to async `Mutex`/`RwLock` and timer utilities in `src/platform`.
- [ ] Update or create examples demonstrating async application initialisation, including wasm-specific guidance. *(App example updated; still need broader guidance in docs.)*
- [ ] If downstream modules break during the refactor, gate or comment out their imports/constructors with `// TODO(async-wasm): re-enable …` and track them in the plan.
- [ ] Add unit tests (native) and wasm-bindgen smoke tests that cover async initialisation, configuration loading, and dependency injection.

## Stage 2 – Identity Backbone (`src/auth`, `src/app_check`, `src/messaging`)
- [x] Refactor `src/auth` so the public API returns futures, backed by async transports split into `auth/api/native.rs` and `auth/api/wasm.rs` implementing a shared trait.
- [x] Implement async token refresh using `gloo_timers` (wasm) and runtime-specific timers (native). Remove legacy blocking wrappers as part of the major bump.
- [x] Introduce an async token provider trait in `src/platform/token.rs`, ensuring `auth`, `app_check`, and `messaging` consume and expose tokens through the shared abstraction.
- [x] Adapt shared messaging transport and push-subscription logic to async primitives, adding wasm feature gates where browser-only facilities are required.
- [x] Update `app_check` to use async HTTP clients and timers, aligning with the token provider contract and annotating wasm-only behaviour.
- [x] Ensure Stage 2 modules compile alongside Stage 1 even if other modules remain temporarily disabled. Document any TODO gates introduced.
  - 2025-02-14: Re-enabled `app_check` for wasm targets; persistence gracefully downgrades to in-memory when the optional `experimental-indexed-db` feature is disabled so the module now participates in wasm builds.
- [ ] Add targeted unit tests and wasm-bindgen tests covering token issuance, messaging registration, and app check attestation workflows. *(Native tests updated; wasm coverage still to expand.)*

## Stage 3 – Feature Modules (Firestore, Storage, Installations, Remote Config, etc.)
- [x] Replace the Storage transport with the unified async client, ensuring operations return futures with Firebase JS naming. (Completed – audit remaining call sites for compliance.)
- [x] Firestore: migrate to async token/provider infrastructure, remove blocking HTTP, and gate wasm-specific paths. (Completed – datastore/client APIs now async.)
- [x] Storage: sweep call sites, README, and examples to reflect async usage, double-check wasm guards.
- [x] Installations: polish async APIs (concurrency coordination, retry/backoff, `onIdChange`) now that the core async client is in place and shared with Messaging/App Check.
  - 2025-02-14: Async APIs stabilized across native/wasm builds with IndexedDB stubs, shared runtime integration, and wasm persistence tests; remaining feature parity items tracked in `src/installations/README.md`.
- [x] Remote Config: adopt the async client/runtime once Installations is ready.
  - 2025-02-14: Converted `get_remote_config`/`fetch` to async and re-enabled the module for wasm. Async HTTP clients now exist for both native (`HttpRemoteConfigFetchClient`) and wasm (`WasmRemoteConfigFetchClient`).
  - 2025-02-14: Added focused transport tests that exercise the native client against mock HTTP responses and verified wasm request shaping under `wasm-bindgen-test`; remaining parity work recorded in `src/remote_config/README.md`.
- [ ] Rework Realtime Database client to use shared async transport, including streaming listeners, exponential backoff, and wasm-compatible long polling/fetch fallbacks.
  - 2025-02-14: Converted the REST backend to async reqwest and kept wasm builds on the in-memory fallback.
  - 2025-02-14: Wired Database listener lifecycle to pause/resume the (placeholder) realtime repo; go_online/go_offline now exist for future transport work.
  - TODO: Implement native WebSocket transport (e.g. tokio-tungstenite), proxy remote events into dispatch_listeners, port OnDisconnect/transactions, and add wasm WebSocket/long-poll support.
  - 2025-02-14: Re-enabled the module for wasm builds using the in-memory backend; REST transport and realtime features remain native-only until the async port lands.
  - 2025-02-14: Converted the REST backend to async `reqwest` and added async `DatabaseReference` helpers; wasm still falls back to in-memory until token handling and HTTP support land.
- [ ] Update Functions, Analytics, and other remaining modules to use the shared async HTTP client and timers, gating wasm-incompatible features with clear documentation.
- [ ] When a module cannot yet compile under wasm, comment out the exposing `pub use` or feature flags with `// TODO(async-wasm): implement wasm-safe pathway` to keep the workspace compiling.
- [ ] Ensure token acquisition hooks (`auth_token`, `app_check_token`) are fully async across the board and document any intentionally unsupported scenarios.

## Stage 4 – Documentation, Examples, Release & CI
- [ ] Update every module README with async usage patterns, wasm caveats, and references to the shared runtime/executor expectations.
- [ ] Add minimal wasm examples under `examples/` that demonstrate acquiring credentials and performing Storage/Database operations inside an async context.
- [ ] Write an upgrade guide outlining breaking changes from the previous major version, including executor requirements and naming parity notes.
- [ ] Document how to run wasm-targeted tests and reference any external tooling (e.g. npm bundlers) needed for integration scenarios.
- [ ] Extend CI to run `cargo check --target wasm32-unknown-unknown --features wasm-web` and the native test matrix on every pull request.
- [ ] Add `wasm-bindgen-test` (or equivalent) suites covering storage transport, timeout behaviour, auth token refresh logic, and app initialisation.
- [ ] Monitor binary size impacts of wasm features and document mitigation strategies if thresholds are exceeded.

## Open Questions / Risks
- **Executor ownership**: Decide whether the library ships an embedded executor (e.g. `tokio` feature default) or requires consumers to bring their own on native builds.
- **Transport parity**: Confirm `reqwest`'s wasm support is sufficient for long-lived connections (Realtime Database, Firestore listeners); otherwise plan for `gloo_net` or custom fetch wrappers.
- **Testing infrastructure**: Determine the minimal tooling for wasm tests (headless browser vs. wasmtime) and how to run them in CI without external services.
- **API breakage communication**: With the removal of sync wrappers, ensure upgrade guides and semantic versioning communicate the breaking changes clearly.
- **Dependency compatibility**: Replace or gate crates that cannot compile to wasm (e.g. `httpmock`, file-system based caches) and provide alternatives where necessary.

## Deliverables Per Stage
- **Stage 0**: Documented wasm toolchain instructions, dependency compatibility matrix, and a reproducible smoke-test command for contributors.
- **Stage 1**: `src/app` exposes async-first APIs, examples/tests updated, and any temporary module exclusions annotated with TODO markers.
- **Stage 2**: Auth/App Check/Messaging share the async token provider and transports, with wasm-compatible implementations and tests.
- **Stage 3**: Remaining modules adopt the async transport, update docs/examples, and use feature gating or TODO annotations for unfinished wasm paths.
- **Stage 4**: Updated READMEs, wasm examples, upgrade guide, and CI coverage for wasm builds/tests alongside native suites.

Progress should be tracked in this document; check boxes as work completes and append notes/links to PRs for traceability.


## End of day wrap up

Wrapping up for today. Here’s what we accomplished and what’s next on the async/WASM front for Realtime Database:

### ✅ Today’s Progress
- Database unified to async: DatabaseReference and DatabaseQuery now expose async methods (e.g., set, get, update,
remove, set_with_priority, push) that await the async REST backend. Synchronous wrappers have been removed; direct
callers now await the asynchronous operations.
- Async REST backend: The REST backend (src/database/backend.rs) uses the async reqwest client, boxed token fetchers,
and guardable behaviour so wasm still falls back to the in-memory backend while native builds hit the network.
- Realtime scaffolding: Introduced a Repo abstraction with a pluggable RealtimeTransport trait and no‑op transport.
Database automatically calls go_online when the first listener is registered and go_offline when the last one is
removed, providing hooks for future WebSocket/long-poll transports.
- Documentation & plan updates: README and WASM plan now reflect the async/wasm status, noting the missing features
(realtime streaming, WebSockets, OnDisconnect, transactions).


### 🚧 What’s Next (Database & Realtime)
1. Implement native WebSocket transport
    - ✅ Native builds now spin up a `tokio_tungstenite` connection in the background when the first listener registers, clearing the sink when the socket closes so reconnects can occur.
    - Handle authentication, message framing, and reconnect logic (basic stubs to start).
    - Feed incoming events into dispatch_listeners so remote updates reach value/child callbacks.
2. Provide wasm transport
    - ✅ wasm builds now receive the same listener spec machinery plus a `web_sys::WebSocket` URL builder; wire up the actual browser transport and fallbacks next.
    - Use web_sys::WebSocket (or gloo-net) to connect from wasm builds, with a long-polling fallback if necessary.
    - Ensure the transport interface (RealtimeTransport) works cross-platform.
3. Hook Repo into Database operations
    - ✅ Database listener registration now routes through `Repo::listen`/`unlisten`, reference-counting targets and toggling the transport automatically during `go_online` / `go_offline`.
    - Extend OnDisconnect and run_transaction to use the new transport (currently return errors).
4. Docs & tests
    - Update README once streaming is live. Current docs mention the new transport scaffolding and listener bookkeeping.
    - Add integration tests (native/wasm) for listener behaviour once a real transport is wired in.
    - Ensure wasm docs clearly state any feature flags (e.g., WebSocket dependencies) and fallback behaviour.

The WASM plan now tracks these remaining steps under Stage 3 so we can pick up exactly where we left off next session.
Let me know when you’re ready to dive back in!
