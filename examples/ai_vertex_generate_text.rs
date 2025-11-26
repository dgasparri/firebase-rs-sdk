//! Vertex AI region setup with limited-use App Check tokens and custom base URL/timeout, normalizing the model name via GenerativeModel

use std::time::Duration;

use firebase_rs_sdk::ai::{get_ai, AiOptions, Backend, GenerateTextRequest, GenerativeModel, RequestOptions};
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

    let ai = get_ai(
        Some(app),
        Some(AiOptions {
            backend: Some(Backend::vertex_ai("europe-west4")),
            use_limited_use_app_check_tokens: Some(true),
        }),
    )
    .await?;

    let request_options = RequestOptions {
        timeout: Some(Duration::from_secs(20)),
        // Point to an emulator or proxy if desired.
        base_url: Some("http://localhost:8080".into()),
    };
    let model = GenerativeModel::new(ai.clone(), "gemini-1.5-flash", Some(request_options.clone()))?;

    let response = ai
        .generate_text(GenerateTextRequest {
            prompt: "List three lunch ideas near the office".to_owned(),
            model: Some(model.model().to_owned()),
            request_options: Some(request_options),
        })
        .await?;

    println!(
        "Vertex AI region {} model {} replied:\n{}",
        ai.location().unwrap_or("unknown"),
        response.model,
        response.text
    );

    Ok(())
}
