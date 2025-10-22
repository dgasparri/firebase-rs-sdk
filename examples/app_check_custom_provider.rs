use firebase_rs_sdk::app::*;
use firebase_rs_sdk::app_check::*;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    // Configure the Firebase project. Replace these placeholder values with your
    // own Firebase configuration when running the sample against real services.
    let options = FirebaseOptions {
        api_key: Some("YOUR_WEB_API_KEY".into()),
        project_id: Some("your-project-id".into()),
        app_id: Some("1:1234567890:web:abcdef".into()),
        ..Default::default()
    };

    let settings = FirebaseAppSettings {
        name: Some("app-check-demo".into()),
        automatic_data_collection_enabled: Some(true),
    };

    let app = initialize_app(options, Some(settings))?;

    // Create a simple provider that always returns the same demo token.
    let provider = custom_provider(|| token_with_ttl("demo-app-check", Duration::from_secs(60)));
    let options = AppCheckOptions::new(provider.clone()).with_auto_refresh(true);

    let app_check = initialize_app_check(Some(app.clone()), options)?;

    // Enable or disable automatic background refresh.
    set_token_auto_refresh_enabled(&app_check, true);

    // Listen for token updates. The listener fires immediately with the cached token
    // (if any) and then on subsequent refreshes.
    let listener: AppCheckTokenListener = Arc::new(|result| {
        if !result.token.is_empty() {
            println!("Received App Check token: {}", result.token);
        }
        if let Some(error) = &result.error {
            eprintln!("App Check token error: {error}");
        }
    });
    let handle = add_token_listener(&app_check, listener, ListenerType::External)?;

    // Retrieve the current token and a limited-use token.
    let token = get_token(&app_check, false)?;
    println!("Immediate token fetch: {}", token.token);

    let limited = get_limited_use_token(&app_check)?;
    println!("Limited-use token: {}", limited.token);

    // Listener handles implement Drop and automatically unsubscribe, but you can
    // explicitly disconnect if desired.
    handle.unsubscribe();

    delete_app(&app)?;
    Ok(())
}
