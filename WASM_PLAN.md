# Auth + Async/WASM Migration Plan

This plan captures the steps required to make the Auth module (and dependent crates) fully compatible with asynchronous execution and the `wasm32-unknown-unknown` target. The work is broken into phases so we can deliver incremental value while keeping the codebase stable.

## Phase 0 – Tooling Prerequisites
- [ ] Ensure the repository builds for `wasm32-unknown-unknown` using `cargo check --target wasm32-unknown-unknown --features wasm-web`. This will require:
  - [ ] Adding the `wasm32-unknown-unknown` target locally (`rustup target add wasm32-unknown-unknown`).
  - [ ] Verifying all dependencies (e.g. `getrandom`, `reqwest`) have the proper wasm features enabled or replaced with wasm-friendly crates.
  - [ ] Documenting the exact feature flags/commands that contributors should run.

## Phase 1 – Auth Core Separation
- [ ] Isolate blocking Auth functionality into a `native` backend and introduce an `async` backend for wasm builds.
  - [ ] Refactor `src/auth/api.rs` into a facade that delegates to `api/native.rs` (current blocking code) or `api/wasm.rs` (new async code) behind cfg guards.
  - [ ] Implement the async REST client using `reqwest::Client` (wasm-compatible features only) or `gloo_net` if `reqwest`’s wasm support remains insufficient.
  - [ ] Replace background `std::thread::spawn` refresh logic with on-demand refresh (or an async timer using `gloo_timers::future::TimeoutFuture`) for wasm.
  - [ ] Maintain synchronous public APIs (`FirebaseAuth` wrappers) for native builds; expose async counterparts that are compiled only on wasm.
  - [ ] Add targeted unit tests for the async backend (possibly using `wasm-bindgen-test` when available).

## Phase 2 – Token Provider Abstraction
- [ ] Update the shared token provider contracts so downstream modules can consume async tokens when compiled for wasm.
  - [ ] Introduce an async trait (e.g. `async_trait`-based) that exposes `get_token(&self) -> Future<Result<Option<String>>>`.
  - [ ] Provide adapter shims so native modules can continue calling synchronously (e.g. by blocking on the future within `tokio::runtime::Runtime` or reusing the existing blocking API).
  - [ ] Revisit `AuthTokenProvider`, `FirebaseAppCheckInternal::token_provider`, and Firestore/Storage auth hooks so they follow the new abstraction without breaking native builds.

## Phase 3 – Storage & Database HTTP Stack
- [ ] Refactor Storage and Realtime Database modules, which currently rely on `reqwest::blocking`, to support both native and wasm clients.
  - [x] Replace the storage transport with a unified async `reqwest::Client`, using Tokio on native targets and `fetch` via `reqwest` on wasm (`gloo-timers` for retry delays). This change intentionally disrupts the previous blocking public API: storage operations now return futures, aligning with wasm expectations. It is the caller’s responsibility to drive the async runtime. Function names follow the Firebase JS SDK naming (no `_async` suffix) even when they return futures.
  - [ ] Convert the remaining database transport to async in the same style, removing blocking entry points and reusing the shared backoff logic.
  - [ ] Rewrite token acquisition paths (`auth_token`, `app_check_token`) to operate asynchronously on wasm; where unsupported, document the limitation and disable signed requests.
  - [ ] Revisit tests that use `httpmock` (which binds to localhost). For wasm builds, replace them with pure in-process mocks or flag them as native-only.

## Phase 4 – Firestore & Messaging Integration
- [ ] Audit Firestore, Messaging, Functions, and Remote Config to ensure they either:
  - [ ] Participate in the async token provider abstraction introduced in Phase 2, and/or
  - [ ] Are explicitly gated off for wasm builds with clear compile-time errors and documentation.
  - [ ] Provide wasm stubs where full functionality is impractical (e.g. Messaging service workers).

## Phase 5 – Documentation & Examples
- [ ] Update module READMEs to describe the async API surface and wasm caveats.
- [ ] Add minimal wasm examples (e.g. embedded in `examples/`) that demonstrate acquiring tokens and making a REST call.
- [ ] Document how to build/run wasm tests, including any required npm tooling or bundlers.
- [ ] Call out explicitly that the library now uses async-first APIs (mirroring Firebase JS naming without `_async` suffixes) and that users must provide an executor when running on native platforms.

## Phase 6 – CI & Regression Testing
- [ ] Configure CI jobs that run `cargo check --target wasm32-unknown-unknown --features wasm-web`.
- [ ] Evaluate using `wasm-pack test` (or `wasm-bindgen-test`) for core logic that can run in headless wasm environments.
- [ ] Ensure native test suites continue to pass after async refactors by adding targeted regression tests for the new async pathways.

## Open Questions / Risks
- **Dependency compatibility**: Some crates (e.g. `httpmock`, `reqwest::blocking`) are inherently native. We need strategy for wasm alternatives.
- **Runtime availability**: For async on native, we may need a consistent runtime (Tokio) or ensure callers inject an executor. Decide whether to expose async-only APIs and let consumers handle runtimes.
- **Binary size/performance**: Introducing wasm support adds `wasm-bindgen`/`js-sys` dependencies. We should monitor build artifact sizes.
- **Phased rollout**: Determine whether to land Phase 1 behind a feature flag to avoid breaking downstream crates before other phases are complete.

## Deliverables Per Phase
- **Phase 0**: Documented instructions for wasm toolchain; dependency gating validated.
- **Phase 1**: `Auth` builds and runs on wasm (tests behind feature flag); native behavior unchanged.
- **Phase 2**: Async token provider trait in place; native modules still compile.
- **Phase 3**: Storage/Database expose async-only APIs that mirror Firebase JS naming (no `_async` suffix), with callers responsible for runtime management; wasm builds share the same transport as native.
- **Phase 4**: Firestore/Messaging pathways audited with either async support or wasm guards.
- **Phase 5**: Updated README + wasm example.
- **Phase 6**: CI coverage for wasm builds.

## Next Options / Focus Areas
1. Sweep remaining storage call sites and examples to adopt the async API (no `_async` naming), add a dedicated async example, and document runtime requirements for native consumers.
2. Tackle the Realtime Database transport, replacing blocking `reqwest` usage with the shared async client and mirroring Firebase JS function names.
3. Add wasm-targeted regression tests (e.g. `wasm-bindgen-test`) that exercise the new async storage transport and timeout behaviour.

Progress should be tracked in this document; check boxes as work completes and append notes/links to PRs for traceability.
