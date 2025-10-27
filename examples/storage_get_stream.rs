//! Example showing how to stream a Firebase Storage object without buffering it entirely in memory.
//!
//! Adjust the bucket name and object path to point at your project before running.

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use firebase_rs_sdk::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};
    use firebase_rs_sdk::storage::get_storage_for_app;
    use tokio::fs::File;
    use tokio::io::{copy, AsyncWriteExt};

    // Configure the Firebase app with your storage bucket.
    let options = FirebaseOptions {
        storage_bucket: Some("your-project.appspot.com".into()),
        ..Default::default()
    };

    let app = initialize_app(options, Some(FirebaseAppSettings::default())).await?;
    let storage = get_storage_for_app(Some(app), None).await?;

    // Reference the object you want to download.
    let reference = storage
        .root_reference()? // Replace with `ok_or` if no default bucket is configured.
        .child("path/to/object.bin");

    // Request a streaming response.
    let response = reference.get_stream(None).await?;
    println!("HTTP {}", response.status);

    // Stream the bytes into a local file.
    let mut reader = response.reader;
    let mut file = File::create("object.bin").await?;
    copy(&mut reader, &mut file).await?;
    file.flush().await?;

    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {
    // Streaming downloads currently require the native target.
    eprintln!("Run this example on a native target (not wasm32).");
}
