# Firebase Performance

## From v1.27.1

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