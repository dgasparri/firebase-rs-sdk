//! Attach custom signals to Remote Config fetches to target experiments or audiences.
//!
//! The custom signals mirror the JS SDK semantics: non-string values are stringified on
//! the wire, and passing `serde_json::Value::Null` removes an existing signal. Provide
//! real Firebase credentials before running so the fetch call can reach the backend.

use std::collections::HashMap;

use firebase_rs_sdk::app::initialize_app;
use firebase_rs_sdk::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::remote_config::RemoteConfigSettingsUpdate;
use firebase_rs_sdk::remote_config::{get_remote_config, CustomSignals};
use serde_json::json;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = FirebaseOptions {
        api_key: Some("AIza_your_api_key".into()),
        project_id: Some("your-project-id".into()),
        app_id: Some("1:1234567890:web:abc123def456".into()),
        ..Default::default()
    };

    let app = initialize_app(options, Some(FirebaseAppSettings::default())).await?;
    let remote_config = get_remote_config(Some(app.clone())).await?;

    // Lower the throttling window for development so we can issue consecutive fetches.
    remote_config.set_config_settings(RemoteConfigSettingsUpdate {
        fetch_timeout_millis: Some(30_000),
        minimum_fetch_interval_millis: Some(0),
    })?;

    let signals: CustomSignals = HashMap::from([
        (String::from("audience"), json!("beta-testers")),
        (String::from("app_version"), json!(42)),
        (String::from("supports_rust"), json!(true)),
    ]);
    remote_config.set_custom_signals(signals).await?;

    remote_config.fetch_and_activate().await?;

    let current_signals = remote_config
        .custom_signals()
        .unwrap_or_else(|| HashMap::from([(String::from("audience"), json!("beta-testers"))]));

    println!("Custom signals sent with fetch: {current_signals:?}");
    println!("theme value from Remote Config: {}", remote_config.get_string("theme"));

    Ok(())
}
