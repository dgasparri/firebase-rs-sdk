## Porting status

- performance 70% `[#######..]`

==As of November 8th, 2025==

The Rust crate supports component registration, configurable runtime settings, manual traces (metrics, attributes,
recording), WASM-friendly time sources, network request instrumentation, App Check/App/Auth integrations, IndexedDB or
file-backed trace queues, a background transport worker, and browser auto instrumentation hooks guarded by the
`wasm-web` feature. Upcoming work focuses on backend sampling, remote configuration, and richer analytics/reporting
parity with the JavaScript SDK.


## Implemented

- **Component registration & initialization parity** – `register_performance_component`, `get_performance`, and
  `initialize_performance` mirror the JS SDK, including the optional `PerformanceSettings` struct, `is_supported`, and
  a new `configure_transport` builder for runtime transport tuning (`src/performance/api.rs`).
- **Configurable runtime toggles** – `PerformanceSettings`/`PerformanceRuntimeSettings` honour the app's automatic
  collection defaults, expose setters, and allow attaching Firebase Auth/App Check instances so traces include user IDs
  and security tokens across modules.
- **Full-featured manual traces** – `TraceHandle` exposes metrics, attributes, increments, and the `record` helper while
  validation logic mirrors the JavaScript SDK. Network instrumentation records payload sizes, status codes, and
  App Check tokens through `NetworkTraceHandle` (`src/performance/api.rs`).
- **Persistent trace queue** – A cross-platform `TraceStore` persists trace and network envelopes to IndexedDB (wasm)
  or a JSONL file (native), feeding an async transport worker built on the shared runtime helpers
  (`src/performance/storage.rs`, `src/performance/transport.rs`).
- **Auto instrumentation for wasm** – When the `wasm-web` feature is enabled, a browser observer captures navigation
  timings and resource fetches so WASM builds gain out-of-the-box traces just like the JS SDK
  (`src/performance/instrumentation.rs`).
- **Docs & tests** – README/quick-start were updated, rustdoc examples reference the new APIs, and async tests cover
  trace recording, network instrumentation, persistence, and (optionally) transport flushing to custom endpoints.

## Still to do

- Remote config & sampling: integrate the remote config service plus sampling helpers so data collection obeys backend
  toggles and rate limits (including per-trace/network sampling).
- Backend transport polish: add Firelog batching semantics (Firelog proto envelope, retries, throttling, exponential
  backoff) and integrate Installations auth tokens plus response handling.
- Browser auto instrumentation depth: capture additional web vitals (FID, LCP, INP) and hook into fetch/XMLHttpRequest
  APIs for parity with the JS `NetworkRequestTrace` helpers.
- Diagnostics & logging: port the perf logger utilities, verbose console hooks, and structured error surface for
  backend upload failures.
- Testing parity: expand integration tests that combine traces, auto instrumentation, and transport to mirror the JS
  fixtures (wasm and native targets).

## Next Steps - Detailed Completion Plan

1. **Remote config + sampling integration**
   - Mirror `SettingsService`/Remote Config plumbing so sampling rates and logging flags can be tuned at runtime.
   - Surface APIs to inspect effective sampling + honour remote-config TTLs, with coverage tests using mocked Remote
     Config responses.
2. **Transport hardening**
   - Implement Firelog batching semantics (payload shaping, retries, throttling, heartbeat) and add structured logging
     for upload failures.
   - Attach Installations auth tokens and App Check headers to outbound payloads and assert against golden fixtures.
3. **Browser instrumentation depth**
   - Expand the WASM instrumentation bridge to capture paint metrics (FP/FCP/LCP/INP/CLS) and wrap fetch/XHR hooks so
     automatic network tracing reaches parity with the JS SDK.
