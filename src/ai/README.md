# Firebase AI 

This module hosts the Rust port of the experimental Firebase AI client. It exposes the same high-level entry points as the JavaScript SDK (`getAI`, backend configuration helpers) while adopting idiomatic Rust patterns for component registration, backend selection, and error handling. Runtime inference is currently stubbed so the surrounding Firebase app infrastructure can already depend on the public API.

Porting status: 30% `[###       ]` ([detail](https://github.com/dgasparri/firebase-rs-sdk/blob/main/src/ai/PORTING_STATUS.md))

## Quick Start Example

```rust,no_run
use firebase_rs_sdk::ai::{get_ai, AiOptions, Backend, GenerateTextRequest};
use firebase_rs_sdk::app::initialize_app;
use firebase_rs_sdk::app::{FirebaseAppSettings, FirebaseOptions};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = initialize_app(
        FirebaseOptions {
            api_key: Some("fake-key".into()),
            project_id: Some("demo-project".into()),
            app_id: Some("1:123:web:abc".into()),
            ..Default::default()
        },
        Some(FirebaseAppSettings::default()),
    )
    .await?;

    let ai = get_ai(
        Some(app),
        Some(AiOptions {
            backend: Some(Backend::vertex_ai("us-central1")),
            use_limited_use_app_check_tokens: Some(false),
        }),
    )
    .await?;

    let response = ai
        .generate_text(GenerateTextRequest {
            prompt: "Hello Gemini!".to_owned(),
            model: None,
            request_options: None,
        })
        .await?;
    println!("{}", response.text);

    Ok(())
}
```

## References to the Firebase JS SDK

- QuickStart: <https://firebase.google.com/docs/ai-assistance/gemini-in-firebase/set-up-gemini>
- API: <https://firebase.google.com/docs/reference/js/ai.md#ai_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/ai>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/ai>
