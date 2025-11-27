//! Typed request/response example for HTTPS callable Functions.
//! Shows how to map Rust structs to JSON payloads without manual serialization code at call sites.

use firebase_rs_sdk::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::functions::{get_functions, register_functions_component};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct AddRequest {
    a: i64,
    b: i64,
}

#[derive(Debug, Deserialize)]
struct AddResponse {
    sum: i64,
}

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

    let functions = get_functions(Some(app.clone()), Some("europe-west1")).await?;
    let add = functions.https_callable::<AddRequest, AddResponse>("addNumbers")?;

    let payload = AddRequest { a: 5, b: 7 };
    let response = add.call_async(&payload).await?;
    println!("5 + 7 = {}", response.sum);

    Ok(())
}
