//! Demonstrates consent defaults, default event parameters, and collection toggles.
//! Events are recorded locally even when collection is disabled.

use std::collections::BTreeMap;

use firebase_rs_sdk::analytics::{get_analytics, AnalyticsSettings, ConsentSettings};
use firebase_rs_sdk::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = FirebaseOptions {
        project_id: Some("your-project-id".into()),
        measurement_id: Some("G-analytics-demo".into()),
        ..Default::default()
    };
    let app = initialize_app(options, Some(FirebaseAppSettings::default())).await?;
    let analytics = get_analytics(Some(app)).await?;

    analytics.set_default_event_parameters(BTreeMap::from([("currency".to_string(), "USD".to_string())]));

    analytics.set_consent_defaults(ConsentSettings {
        entries: BTreeMap::from([("ad_storage".to_string(), "denied".to_string())]),
    });

    analytics.apply_settings(AnalyticsSettings {
        config: BTreeMap::from([("send_page_view".to_string(), "false".to_string())]),
        send_page_view: Some(false),
    });

    // Pause outbound collection while still recording local events.
    analytics.set_collection_enabled(false);

    let mut params = BTreeMap::new();
    params.insert("engagement_time_msec".to_string(), "200".to_string());
    params.insert("level".to_string(), "1".to_string());
    analytics.log_event("level_up", params).await?;

    println!("Recorded events: {:?}", analytics.recorded_events());
    println!("Gtag bootstrap state: {:?}", analytics.gtag_state());

    Ok(())
}
