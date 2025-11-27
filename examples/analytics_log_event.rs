//! Minimal Analytics example that records an event locally without configuring any transport.
//! Replace the placeholders with your Firebase project details if you want to resolve the
//! measurement ID automatically.

use std::collections::BTreeMap;

use firebase_rs_sdk::analytics::get_analytics;
use firebase_rs_sdk::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = FirebaseOptions {
        project_id: Some("your-project-id".into()),
        measurement_id: Some("G-1234567890".into()),
        ..Default::default()
    };
    let app = initialize_app(options, Some(FirebaseAppSettings::default())).await?;
    let analytics = get_analytics(Some(app)).await?;

    let mut params = BTreeMap::new();
    params.insert("engagement_time_msec".to_string(), "100".to_string());
    params.insert("tutorial_name".to_string(), "first_steps".to_string());

    analytics.log_event("tutorial_begin", params).await?;

    for event in analytics.recorded_events() {
        println!("Recorded event: {} {:?}", event.name, event.params);
    }

    Ok(())
}
