# Firebase Performance Module

## Introduction

This module contains the Rust port of the Firebase Performance Monitoring SDK. The current implementation wires the
`performance` component into the shared container and offers a lightweight, in-memory trace recorder so other services
can depend on it while the full feature set is being ported.

## Porting status

- performance 5% `[##        ]`

==As of October 21st, 2025==

The Rust crate currently exposes component registration and manual trace recording. The JavaScript SDK includes automatic
instrumentation, backend upload, installations integration, sampling, remote configuration, and logging utilities that
remain to be implemented.

## Quick Start Example

```rust,no_run
use std::time::Duration;

use firebase_rs_sdk::app::api::initialize_app;
use firebase_rs_sdk::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::performance::get_performance;

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
    trace.put_metric("items_rendered", 5)?;
    tokio::time::sleep(Duration::from_millis(25)).await;
    let trace_data = trace.stop().await?;

    println!(
        "trace '{}' completed in {:?} with metrics {:?}",
        trace_data.name, trace_data.duration, trace_data.metrics
    );

    Ok(())
}
```

## Implemented

- **Component registration** – `register_performance_component` exposes the `performance` component so
  `get_performance` can asynchronously resolve instances through the shared container (`src/performance/api.rs`).
- **Async trace handles** – `Performance::new_trace` returns a `TraceHandle` whose `stop` method is `async` and stores
  results via an `async_lock::Mutex`, making the API cooperative on wasm targets.
- **In-memory trace store** – Recorded traces are kept in memory and can be retrieved asynchronously with
  `Performance::recorded_trace` for inspection during tests or debugging.
- **Error surface** – Minimal error codes (`performance/invalid-argument`, `performance/internal`) mirroring the JS SDK.
- **Unit tests** – Async test covering trace recording, metric storage, and retrieval.

## Still to do

- Backend transport: collect traces and upload them to the Performance Monitoring backend, including installations token
  integration and throttling logic.
- Automatic instrumentation: port page-load, network request, and resource timing instrumentation found in the
  JavaScript SDK.
- Trace lifecycle: support attributes, custom metrics, increment APIs, session handling, and end-to-end sampling.
- Settings & sampling: integrate remote config toggles, rate limiting, and logging controls.
- Platform guards: replicate browser environment checks (e.g., `isSupported`) and add stubs for non-web targets.
- Testing parity: port the JS test suites for traces, network monitoring, transport, and settings validation.

## Next Steps - Detailed Completion Plan

1. **Introduce persistent trace buffering**
   - Add IndexedDB (wasm) and file-based (native) stores so traces survive reloads before upload.
   - Wire the buffers into the async trace recorder and add tests for persistence behaviour.
2. **Implement transport wiring**
   - Port the PerformanceController/Transport service to batch traces, attach installations tokens, and honour backend
     rate limits.
   - Add integration tests that validate request payloads against the JS fixtures.
3. **Expand trace semantics**
   - Implement attribute setters, metric increment APIs, and network request tracing to close the gap with the JS
     `Trace` and `NetworkRequestTrace` abstractions.
   - Extend the README and rustdoc with examples once these features land.
