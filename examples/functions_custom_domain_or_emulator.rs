//! Target a callable through a custom domain or the local emulator.
//! Pass a full origin (and optional path) to `get_functions` to mirror the JS API's custom domain overload.

use firebase_rs_sdk::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::functions::{get_functions, register_functions_component};
use serde_json::json;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    register_functions_component();

    // Point to your emulator or hosting rewrite. Example:
    // - Emulator: http://127.0.0.1:5001/my-project/us-central1
    // - Custom domain: https://functions.example.com
    let custom_domain = std::env::var("FUNCTIONS_ORIGIN")
        .unwrap_or_else(|_| "http://127.0.0.1:5001/my-project/us-central1".to_string());

    let app = initialize_app(
        FirebaseOptions {
            project_id: Some("my-project".into()),
            ..Default::default()
        },
        Some(FirebaseAppSettings::default()),
    )
    .await?;

    // The identifier is interpreted as a domain/URL when it parses as a URL.
    let functions = get_functions(Some(app.clone()), Some(&custom_domain)).await?;
    let callable = functions.https_callable::<serde_json::Value, serde_json::Value>("ping")?;

    let response = callable.call_async(&json!({ "from": "custom-domain" })).await?;
    println!("Callable response: {response}");

    Ok(())
}
