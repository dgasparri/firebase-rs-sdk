//! # Firebase App module
//!
//! This module ports core pieces of the Firebase App SDK to Rust.
//!
//! The Firebase App coordinates the communication between the different Firebase components.
//!
//! ## References to the Firebase JS SDK - firestore module
//!
//! - API: <https://firebase.google.com/docs/reference/js/app.md#app_package>
//! - Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/app>
//! - Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/app>
//!
//! ## Development status as of 14th October 2025
//!
//! - Core functionalities: Mostly implemented (see the module's [README.md](https://github.com/dgasparri/firebase-rs-sdk/tree/main/src/firestore) for details)
//! - Tests: 12 tests (passed)
//! - Documentation: Most public functions are documented
//! - Examples: None provided
//!
//! DISCLAIMER: This is not an official Firebase product, nor it is guaranteed that it has no bugs or that it will work as intended.
//!
//! ## Example Usage
//!
//! ```rust
//! use firebase_rs_sdk::app::*;
//!
//! #[tokio::main(flavor = "current_thread")]
//! async fn main() -> AppResult<()> {
//!     // Configure your Firebase project credentials. These values are placeholders that allow the
//!     // example to compile without contacting Firebase services.
//!     let options = FirebaseOptions {
//!         api_key: Some("demo-api-key".into()),
//!         project_id: Some("demo-project".into()),
//!         storage_bucket: Some("demo-project.appspot.com".into()),
//!         ..Default::default()
//!     };
//!
//!     // Give the app a custom name and enable automatic data collection.
//!     let settings = FirebaseAppSettings {
//!         name: Some("demo-app".into()),
//!         automatic_data_collection_enabled: Some(true),
//!     };
//!
//!     // Create (or reuse) the app instance.
//!     let app = initialize_app(options, Some(settings)).await?;
//!     println!(
//!         "Firebase app '{}' initialised (project: {:?})",
//!         app.name(),
//!         app.options().project_id
//!     );
//!
//!     // You can look the app up later using its name.
//!     let resolved = get_app(Some(app.name())).await?;
//!     println!("Resolved app '{}' via registry", resolved.name());
//!
//!     // The registry can also enumerate every active app.
//!     let apps = get_apps().await;
//!     println!("Currently loaded apps: {}", apps.len());
//!     for listed in apps {
//!         println!(
//!             " - {} (automatic data collection: {})",
//!             listed.name(),
//!             listed.automatic_data_collection_enabled()
//!         );
//!     }
//!
//!     // When finished, delete the app to release resources and remove it from the registry.
//!     delete_app(&app).await?;
//!     println!("App '{}' deleted", app.name());
//!
//!     Ok(())
//! }
//! ```

pub mod api;
mod component;
mod constants;
mod core_components;
mod errors;
mod heartbeat;
mod logger;
mod namespace;
mod platform_logger;
pub mod private;
pub(crate) mod registry;
mod types;

#[doc(inline)]
pub use api::{
    delete_app, get_app, get_apps, initialize_app, initialize_server_app, on_log, register_version,
    set_log_level, SDK_VERSION,
};

#[doc(inline)]
pub use errors::{AppError, AppResult};

#[doc(inline)]
pub use logger::{LogCallback, LogLevel, LogOptions, Logger, LOGGER};

#[doc(inline)]
pub use namespace::FirebaseNamespace;

#[doc(inline)]
pub use types::{
    FirebaseApp, FirebaseAppConfig, FirebaseAppSettings, FirebaseOptions, FirebaseServerApp,
    FirebaseServerAppSettings, VersionService,
};

use std::sync::LazyLock;

pub(crate) fn ensure_core_components_registered() {
    LazyLock::force(&CORE_COMPONENTS_REGISTERED);
}

static CORE_COMPONENTS_REGISTERED: LazyLock<()> = LazyLock::new(|| {
    core_components::ensure_registered();
});
