//! Shows how to prepare a generateContent request and execute it with a custom reqwest client.

use firebase_rs_sdk::ai::{get_ai, internal_error, GenerativeModel};
use firebase_rs_sdk::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};
use reqwest::Client;
use serde_json::{json, Value};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = initialize_app(
        FirebaseOptions {
            api_key: Some("your-api-key".into()),
            project_id: Some("your-project-id".into()),
            app_id: Some("your-app-id".into()),
            ..Default::default()
        },
        Some(FirebaseAppSettings::default()),
    )
    .await?;

    let ai = get_ai(Some(app), None).await?;
    let model = GenerativeModel::new(ai.clone(), "gemini-pro", None)?;

    let prepared = model
        .prepare_generate_content_request(
            json!({
                "contents": [
                    {
                        "role": "user",
                        "parts": [{ "text": "Write a haiku about Rust." }]
                    }
                ]
            }),
            None,
        )
        .await?;

    let client = Client::new();
    let response = prepared
        .into_reqwest(&client)?
        .send()
        .await
        .map_err(|err| internal_error(format!("HTTP request failed: {err}")))?
        .json::<Value>()
        .await
        .map_err(|err| internal_error(format!("Failed to decode JSON: {err}")))?;

    println!("{}", serde_json::to_string_pretty(&response).unwrap());

    Ok(())
}
