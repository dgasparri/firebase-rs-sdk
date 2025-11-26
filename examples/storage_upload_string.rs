//! Example showing how to upload a string to Firebase Storage using the `upload_string` helper.
//!
//! Adjust the bucket name and paths before running.

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use firebase_rs_sdk::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};
    use firebase_rs_sdk::storage::{get_storage_for_app, StringFormat};

    let options = FirebaseOptions {
        storage_bucket: Some("your-project.appspot.com".into()),
        ..Default::default()
    };

    let app = initialize_app(options, Some(FirebaseAppSettings::default())).await?;
    let storage = get_storage_for_app(Some(app), None).await?;

    let reference = storage
        .root_reference()? // Replace with explicit bucket path if needed.
        .child("demo/uploaded.txt");

    let content = "Hello from Rust!";
    let metadata = None;

    let metadata = reference.upload_string(content, StringFormat::Raw, metadata).await?;

    println!("Uploaded object: {:?}", metadata.name);
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {
    eprintln!("Run this example on a native target (not wasm32).");
}
