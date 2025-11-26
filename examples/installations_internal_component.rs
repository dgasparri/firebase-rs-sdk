//! Demonstrates resolving the private `installations-internal` component and using it
//! to obtain ID/token data that can be shared with other Firebase services.
//!
//! This example mimics the way Remote Config / Messaging consume Installations.
//! It requires network access and valid Firebase credentials.

use firebase_rs_sdk::app::initialize_app;
use firebase_rs_sdk::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::installations::{delete_installations, get_installations_internal, InstallationToken};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = FirebaseOptions {
        api_key: Some("AIza_your_api_key".into()),
        project_id: Some("your-project-id".into()),
        app_id: Some("1:1234567890:web:abc123def456".into()),
        ..Default::default()
    };

    let app = initialize_app(options, Some(FirebaseAppSettings::default())).await?;

    // Resolve the internal component; this mirrors what other Firebase services do.
    let installations_internal = get_installations_internal(Some(app.clone()))?;
    let installations = firebase_rs_sdk::installations::get_installations(Some(app.clone()))?;

    let fid = installations_internal.get_id().await?;
    println!("Internal component FID: {fid}");

    // Internal component exposes the same token API as the public service.
    let InstallationToken { token, expires_at } = installations_internal.get_token(false).await?;
    println!("Internal auth token: {token}");
    println!("Expires at: {:?}", expires_at);

    // Optionally clean up the installation afterwards.
    delete_installations(&installations).await?;

    Ok(())
}
