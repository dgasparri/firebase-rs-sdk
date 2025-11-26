//! Minimal Google AI prompt-to-text flow using default backend.

use firebase_rs_sdk::ai::{get_ai, GenerateTextRequest};
use firebase_rs_sdk::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};

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
    let response = ai
        .generate_text(GenerateTextRequest {
            prompt: "Say hello from Firebase AI".to_owned(),
            model: None,
            request_options: None,
        })
        .await?;

    println!("Model {} responded:\n{}", response.model, response.text);

    Ok(())
}
