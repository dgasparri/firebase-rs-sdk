//! Fetch the latest Remote Config template, activate it, and read typed values.
//!
//! Replace the placeholder Firebase credentials before running so the example can talk to
//! the Remote Config backend. Set `FIREBASE_REMOTE_CONFIG_LANGUAGE_CODE` if you want to
//! override the default `en-US` language hint.

use std::collections::HashMap;

use firebase_rs_sdk::app::initialize_app;
use firebase_rs_sdk::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::remote_config::get_remote_config;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // These values come from your Firebase project settings.
    let options = FirebaseOptions {
        api_key: Some("AIza_your_api_key".into()),
        project_id: Some("your-project-id".into()),
        app_id: Some("1:1234567890:web:abc123def456".into()),
        ..Default::default()
    };

    let app = initialize_app(options, Some(FirebaseAppSettings::default())).await?;
    let remote_config = get_remote_config(Some(app.clone())).await?;

    // Local defaults apply immediately and are used when the backend has no value for a key.
    remote_config.set_defaults(HashMap::from([
        (String::from("welcome_message"), String::from("Hello from defaults")),
        (String::from("feature_enabled"), String::from("false")),
    ]));

    if remote_config.fetch_and_activate().await? {
        println!("Fetched and activated fresh parameters");
    } else {
        println!("Using cached or default parameters");
    }

    let welcome = remote_config.get_string("welcome_message");
    let feature_enabled = remote_config.get_boolean("feature_enabled");
    let source = remote_config.get_value("welcome_message").source().as_str();

    println!("welcome_message ({source}): {welcome}");
    println!("feature_enabled: {feature_enabled}");

    Ok(())
}
