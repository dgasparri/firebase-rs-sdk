//! Sends analytics events through the GA4 Measurement Protocol.
//! Provide your own measurement ID and API secret from Google Analytics before running.

use std::collections::BTreeMap;

use firebase_rs_sdk::analytics::{get_analytics, MeasurementProtocolConfig, MeasurementProtocolEndpoint};
use firebase_rs_sdk::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = FirebaseOptions {
        project_id: Some("your-project-id".into()),
        measurement_id: Some("G-your-measurement-id".into()),
        app_id: Some("1:1234567890:web:abcdef".into()),
        ..Default::default()
    };
    let app = initialize_app(options, Some(FirebaseAppSettings::default())).await?;
    let analytics = get_analytics(Some(app)).await?;

    // Use the debug endpoint while wiring things up so GA4 returns validation feedback.
    let config = MeasurementProtocolConfig::new("G-your-measurement-id", "your-api-secret")
        .with_endpoint(MeasurementProtocolEndpoint::DebugCollect);
    analytics.configure_measurement_protocol(config)?;
    analytics.set_client_id("example-client-id");

    let mut params = BTreeMap::new();
    params.insert("engagement_time_msec".to_string(), "150".to_string());
    params.insert("method".to_string(), "email".to_string());

    analytics.log_event("login", params).await?;

    Ok(())
}
