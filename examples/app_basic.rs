use firebase_rs_sdk_unofficial::app::api::{delete_app, get_app, get_apps, initialize_app};
use firebase_rs_sdk_unofficial::app::{AppResult, FirebaseAppSettings, FirebaseOptions};

fn main() -> AppResult<()> {
    // Configure your Firebase project credentials. These values are placeholders that allow the
    // example to compile without contacting Firebase services.
    let options = FirebaseOptions {
        api_key: Some("demo-api-key".into()),
        project_id: Some("demo-project".into()),
        storage_bucket: Some("demo-project.appspot.com".into()),
        ..Default::default()
    };

    // Give the app a custom name and enable automatic data collection.
    let settings = FirebaseAppSettings {
        name: Some("demo-app".into()),
        automatic_data_collection_enabled: Some(true),
    };

    // Create (or reuse) the app instance.
    let app = initialize_app(options, Some(settings))?;
    println!(
        "Firebase app '{}' initialised (project: {:?})",
        app.name(),
        app.options().project_id
    );

    // You can look the app up later using its name.
    let resolved = get_app(Some(app.name()))?;
    println!("Resolved app '{}' via registry", resolved.name());

    // The registry can also enumerate every active app.
    let apps = get_apps();
    println!("Currently loaded apps: {}", apps.len());
    for listed in apps {
        println!(
            " - {} (automatic data collection: {})",
            listed.name(),
            listed.automatic_data_collection_enabled()
        );
    }

    // When finished, delete the app to release resources and remove it from the registry.
    delete_app(&app)?;
    println!("App '{}' deleted", app.name());

    Ok(())
}
