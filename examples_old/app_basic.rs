use firebase_rs_sdk::app::*;

#[tokio::main(flavor = "current_thread")]
async fn main() -> AppResult<()> {
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
    let app = initialize_app(options, Some(settings)).await?;
    println!(
        "Firebase app '{}' initialised (project: {:?})",
        app.name(),
        app.options().project_id
    );

    // You can look the app up later using its name.
    let resolved = get_app(Some(app.name())).await?;
    println!("Resolved app '{}' via registry", resolved.name());

    // The registry can also enumerate every active app.
    let apps = get_apps().await;
    println!("Currently loaded apps: {}", apps.len());
    for listed in apps {
        println!(
            " - {} (automatic data collection: {})",
            listed.name(),
            listed.automatic_data_collection_enabled()
        );
    }

    // When finished, delete the app to release resources and remove it from the registry.
    delete_app(&app).await?;
    println!("App '{}' deleted", app.name());

    Ok(())
}
