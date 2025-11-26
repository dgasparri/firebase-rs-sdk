# Firebase Performance

This module contains the Rust port of the Firebase Performance Monitoring SDK. The implementation now exposes the
component through the shared container, provides configurable runtime toggles, implements manual trace plus HTTP
instrumentation primitives, runs WASM-friendly auto instrumentation, and ships a cross-platform trace queue with an
async transport worker so the data path mirrors the JS SDK end-to-end.

Porting status: 70% `[#######..]` ([details](https://github.com/dgasparri/firebase-rs-sdk/blob/main/src/performance/PORTING_STATUS.md))

## Quick Start Example

```rust,no_run
use std::time::Duration;

use firebase_rs_sdk::app::initialize_app;
use firebase_rs_sdk::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::performance::{get_performance, HttpMethod, TransportOptions};

# #[cfg(target_arch = "wasm32")]
# fn main() {}
#[cfg(not(target_arch = "wasm32"))]
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
    performance.configure_transport(TransportOptions {
        endpoint: Some("https://firebaselogging.googleapis.com/v0cc/log?format=json_proto3".into()),
        api_key: app.options().api_key.clone(),
        flush_interval: Some(Duration::from_secs(5)),
        max_batch_size: Some(25),
    });

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

## References to the Firebase JS SDK

- QuickStart: <https://firebase.google.com/docs/perf-mon/get-started-web>
- API: <https://firebase.google.com/docs/reference/js/performance.md#performance_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/performance>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/performance>
