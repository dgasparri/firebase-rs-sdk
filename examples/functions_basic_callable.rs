//! Minimal Functions example invoking a callable with JSON payload.
//! Provide your Firebase project ID and deploy a callable named `helloWorld` (or adjust the name).

use firebase_rs_sdk::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::functions::{get_functions, register_functions_component};
use serde_json::json;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    register_functions_component();

    let app = initialize_app(
        FirebaseOptions {
            project_id: Some("your-project-id".into()),
            ..Default::default()
        },
        Some(FirebaseAppSettings::default()),
    )
    .await?;

    // Uses the default region (us-central1) just like the JS SDK.
    let functions = get_functions(Some(app.clone()), None).await?;
    let callable = functions.https_callable::<serde_json::Value, serde_json::Value>("helloWorld")?;

    let response = callable.call_async(&json!({ "message": "Hello from Rust!" })).await?;
    println!("Callable response: {response}");

    Ok(())
}
