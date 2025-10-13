use crate::app::api;
use crate::app::errors::AppResult;
use crate::app::logger::{LogCallback, LogLevel, LogOptions};
use crate::app::types::{FirebaseApp, FirebaseAppSettings, FirebaseOptions};
use crate::auth::error::{AuthError, AuthResult};
use crate::auth::{self, Auth};
use std::sync::Arc;

pub struct FirebaseNamespace;

impl FirebaseNamespace {
    /// Public entry point mirroring the JS `initializeApp` helper.
    pub fn initialize_app(
        options: FirebaseOptions,
        settings: Option<FirebaseAppSettings>,
    ) -> AppResult<FirebaseApp> {
        api::initialize_app(options, settings)
    }

    /// Returns an initialized `FirebaseApp` by name or the default app when `None` is provided.
    pub fn app(name: Option<&str>) -> AppResult<FirebaseApp> {
        api::get_app(name)
    }

    /// Lists all apps that have been initialized in the current process.
    pub fn apps() -> Vec<FirebaseApp> {
        api::get_apps()
    }

    /// Registers an additional library version for platform logging.
    pub fn register_version(library: &str, version: &str, variant: Option<&str>) {
        api::register_version(library, version, variant)
    }

    /// Updates the global log verbosity for Firebase.
    pub fn set_log_level(level: LogLevel) {
        api::set_log_level(level)
    }

    /// Installs or clears a user-provided log callback.
    pub fn on_log(callback: Option<LogCallback>, options: Option<LogOptions>) -> AppResult<()> {
        api::on_log(callback, options)
    }

    /// Exposes the Firebase SDK version bundled in this crate.
    pub fn sdk_version() -> &'static str {
        api::SDK_VERSION
    }

    /// Provides access to the Auth service for the given (or default) app.
    pub fn auth(app: Option<FirebaseApp>) -> AuthResult<Arc<Auth>> {
        auth::api::register_auth_component();
        let app = match app {
            Some(app) => app,
            None => api::get_app(None).map_err(AuthError::from)?,
        };
        auth::api::auth_for_app(app)
    }
}
