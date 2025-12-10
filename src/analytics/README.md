# Firebase Analytics 

The Analytics module ports the modular `@firebase/analytics` SDK to Rust. It wires into the shared Firebase component
system so other services can obtain an `Analytics` instance that records events and optionally forwards them to Google
Analytics using the GA4 Measurement Protocol. Initialization now mirrors the JS SDK by resolving measurement IDs in the
background and propagating collection toggles to the gtag bootstrap state.

Porting status: 30% `[###        ]` ([details](https://github.com/dgasparri/firebase-rs-sdk/blob/main/src/analytics/PORTING_STATUS.md))


## Quick Start Example

```rust,no_run
use std::collections::BTreeMap;

use firebase_rs_sdk::analytics::{
    get_analytics, MeasurementProtocolConfig, MeasurementProtocolEndpoint,
};
use firebase_rs_sdk::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};


#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = FirebaseOptions {
        api_key: Some("AIza...".into()),
        app_id: Some("1:1234567890:web:abcdef".into()),
        measurement_id: Some("G-1234567890".into()),
        ..Default::default()
    };
    let settings = FirebaseAppSettings {
        name: Some("analytics-demo".into()),
        ..Default::default()
    };

    let app = initialize_app(options, Some(settings)).await?;
    let analytics = get_analytics(Some(app)).await?;

    // Provide the GA4 measurement ID and API secret generated in Google Analytics.
    let config = MeasurementProtocolConfig::new("G-1234567890", "api-secret")
        .with_endpoint(MeasurementProtocolEndpoint::Collect);
    analytics.configure_measurement_protocol(config)?;

    let mut params = BTreeMap::new();
    params.insert("engagement_time_msec".to_string(), "100".to_string());
    analytics.log_event("tutorial_begin", params).await?;

    Ok(())
}
```

## References to the Firebase JS SDK

- QuickStart: <https://firebase.google.com/docs/analytics/get-started?platform=web>
- API: <https://firebase.google.com/docs/reference/js/analytics.md#analytics_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/analytics>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/analytics>
