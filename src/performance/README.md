# Firebase Performance Module

## Introduction

This module contains the Rust port of the Firebase Performance Monitoring SDK. The implementation now exposes the
component through the shared container, provides configurable runtime toggles, and implements manual trace plus HTTP
instrumentation primitives that other services can depend on while backend upload work proceeds.

## Porting status

- performance 40% `[####......]`

==As of November 8th, 2025==

The Rust crate supports component registration, configurable runtime settings, manual traces (metrics, attributes,
recording), WASM-friendly time sources, network request instrumentation, App Check bridging, and helper APIs such as
`initialize_performance`/`is_supported`. Upcoming work focuses on backend upload, persistent storage, and automatic
instrumentation that mirrors the JavaScript SDK.

## Quick Start Example

```rust,no_run
use std::time::Duration;

use firebase_rs_sdk::app::initialize_app;
use firebase_rs_sdk::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::performance::{get_performance, HttpMethod};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = FirebaseOptions {
        project_id: Some("demo-project".into()),
        ..Default::default()
    };
    let settings = FirebaseAppSettings {
        name: Some("demo-app".into()),
        ..Default::default()
    };
    let app = initialize_app(options, Some(settings)).await?;
    let performance = get_performance(Some(app.clone())).await?;

    let mut trace = performance.new_trace("page_load")?;
    trace.start()?;
    trace.put_metric("items_rendered", 5)?;
    tokio::time::sleep(Duration::from_millis(25)).await;
    let trace_data = trace.stop().await?;

    println!(
        "trace '{}' completed in {:?} with metrics {:?}",
        trace_data.name, trace_data.duration, trace_data.metrics
    );

    let mut request = performance
        .new_network_request("https://example.com/api", HttpMethod::Get)?;
    request.start()?;
    // ... perform the actual HTTP call ...
    let http_metrics = request.stop().await?;

    println!(
        "network trace '{}' {:?} us",
        http_metrics.url, http_metrics.time_to_request_completed_us
    );

    Ok(())
}
```

## Implemented

- **Component registration & initialization parity** – `register_performance_component`, `get_performance`, and the new
  `initialize_performance` mirror the JS SDK flow, including the optional `PerformanceSettings` struct and
  `is_supported` guard (`src/performance/api.rs`).
- **Configurable runtime toggles** – `PerformanceSettings` and `PerformanceRuntimeSettings` honour the app's automatic
  data-collection default, expose `set_data_collection_enabled` / `set_instrumentation_enabled`, and allow attaching
  Firebase Auth user IDs and App Check providers so traces carry cross-module context.
- **Full-featured manual traces** – `TraceHandle` now exposes metric increment APIs, attribute setters, the
  `record(start_time, duration, options)` helper, and enforces JS-compatible validation rules.
- **Network instrumentation** – `NetworkTraceHandle` records manual HTTP metadata (payload sizes, response codes,
  response timing, content type) and automatically attaches App Check tokens when available.
- **Docs & tests** – Rustdoc examples cover the new APIs and a suite of async unit tests verifies traces, recording,
  settings application, and network/App Check integration to guarantee WASM-friendly behaviour.

## Still to do

- Backend transport: collect traces and upload them to the Performance Monitoring backend (batching, throttling,
  response handling) while attaching installations/app-check tokens.
- Persistent buffering: store traces in IndexedDB (wasm) and a native-friendly store so events survive reloads before
  upload, matching the JS SDK resiliency story.
- Automatic instrumentation: port out-of-the-box page load, resource timing, and XHR/fetch interception, including
  browser-only guards behind the `wasm-web` feature flag.
- Remote config & sampling: integrate the remote config service plus sampling helpers so data collection obeys backend
  toggles and rate limits.
- Diagnostics & logging: port the JS perf logger utilities, verbose console hooks, and error reporting pipeline.
- Testing parity: expand integration tests that combine traces, transport, and configuration to mirror the JS fixtures.

## Next Steps - Detailed Completion Plan

1. **Persistent queue for upload parity**
   - Add a storage abstraction under `src/platform` that backs onto IndexedDB for wasm and a file/SQLite store for
     native builds.
   - Extend `Performance::store_trace` / `store_network_request` to enqueue records into the persistent store, then add
     fixtures that verify records survive process restarts.
2. **Transport controller and sampling**
   - Port the JS `PerformanceController` + transport service to batch traces, attach installations IDs/App Check tokens,
     and honour throttling rules.
   - Integrate Remote Config lookups so the `PerformanceSettings` runtime toggles pick up backend sampling instructions,
     and cover the flow with async integration tests.
3. **Automatic instrumentation on wasm/web**
   - Introduce a `PerformanceObserver` bridge in `src/platform/browser` to capture page-load/resource entries under the
     `wasm-web` feature, falling back gracefully on native targets.
   - Layer fetch/XHR interception on top of `NetworkTraceHandle`, and add smoke tests that ensure the hooks can be
     enabled/disabled through the runtime settings API.
