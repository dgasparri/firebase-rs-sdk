use crate::app::{FirebaseApp, FirebaseAppConfig, FirebaseOptions};
use crate::component::ComponentContainer;

/// Build a Firebase app with the given API key for use in tests.
///
/// Each call returns a new app wired to its own component container so tests can
/// remain isolated with respect to registered services.
pub fn test_firebase_app_with_api_key(api_key: impl Into<String>) -> FirebaseApp {
    let options = FirebaseOptions {
        api_key: Some(api_key.into()),
        ..Default::default()
    };
    let config = FirebaseAppConfig::new("test", false);
    let container = ComponentContainer::new("test");
    FirebaseApp::new(options, config, container)
}
