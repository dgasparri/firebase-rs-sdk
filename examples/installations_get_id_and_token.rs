//! Run with real Firebase credentials to obtain the Installation ID and auth token.
//!
//! The example uses async APIs and will contact the Firebase Installations REST endpoint.
//! Provide your own API key/project/app id (and optionally set the `FIREBASE_INSTALLATIONS_CACHE_DIR`
//! environment variable if you want to control where the cache lives).

use firebase_rs_sdk::app::api::initialize_app;
use firebase_rs_sdk::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::installations::InstallationToken;
use firebase_rs_sdk::installations::{delete_installations, get_installations};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Replace these placeholders with the credentials from your Firebase project.
    let options = FirebaseOptions {
        api_key: Some("AIza_your_api_key".into()),
        project_id: Some("your-project-id".into()),
        app_id: Some("1:1234567890:web:abc123def456".into()),
        ..Default::default()
    };

    let app = initialize_app(options, Some(FirebaseAppSettings::default()))?;

    // Resolve the Installations service from the component container.
    let installations = get_installations(Some(app.clone()))?;

    // Fetch (or register) the Installation ID.
    let fid = installations.get_id().await?;
    println!("Installation ID: {fid}");

    // Request an auth token; passing `true` forces a refresh instead of reusing the cached token.
    let InstallationToken { token, expires_at } = installations.get_token(true).await?;
    println!("Auth token: {token}");
    println!("Expires at: {:?}", expires_at);

    // Optionally remove the installation from Firebase and clear the local cache.
    // Comment out this line if you want the installation to persist across runs.
    delete_installations(&installations).await?;

    Ok(())
}
