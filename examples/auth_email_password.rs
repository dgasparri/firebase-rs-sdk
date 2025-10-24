use firebase_rs_sdk::app::*;
use firebase_rs_sdk::auth::*;
use std::error::Error;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    // Configure the Firebase project. Replace the placeholder values with your
    // project's credentials before running the example.
    let options = FirebaseOptions {
        api_key: Some("YOUR_WEB_API_KEY".into()),
        project_id: Some("your-project-id".into()),
        auth_domain: Some("your-project-id.firebaseapp.com".into()),
        ..Default::default()
    };

    let settings = FirebaseAppSettings {
        name: Some("auth-demo".into()),
        automatic_data_collection_enabled: Some(true),
    };

    // Initialise the core Firebase App instance.
    let app = initialize_app(options, Some(settings)).await?;

    // Ensure the Auth component is registered so `auth_for_app` succeeds.
    register_auth_component();
    let auth = auth_for_app(app.clone())?;

    // Replace these with credentials recognised by your Firebase project.
    let email = "alice@example.com";
    let password = "correct-horse-battery-staple";

    let credential = auth
        .sign_in_with_email_and_password(email, password)
        .await?;
    println!(
        "Signed in as {} (provider: {:?})",
        credential.user.uid(),
        credential.provider_id
    );

    if let Some(current_user) = auth.current_user() {
        println!(
            "Current user email: {:?}",
            current_user.info().email.clone()
        );
    }

    // Sign the user out and clean up the app instance when finished.
    auth.sign_out();
    println!("Signed out.");

    firebase_rs_sdk::app::api::delete_app(&app).await?;
    println!("App deleted.");

    Ok(())
}
