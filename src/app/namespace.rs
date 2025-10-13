use crate::app::api;
use crate::app::errors::AppResult;
use crate::app::logger::{LogCallback, LogLevel, LogOptions};
use crate::app::types::{FirebaseApp, FirebaseAppSettings, FirebaseOptions};
use crate::auth::error::{AuthError, AuthResult};
use crate::auth::{self, Auth};
use std::sync::Arc;

pub struct FirebaseNamespace;

impl FirebaseNamespace {
    pub fn initialize_app(
        options: FirebaseOptions,
        settings: Option<FirebaseAppSettings>,
    ) -> AppResult<FirebaseApp> {
        api::initialize_app(options, settings)
    }

    pub fn app(name: Option<&str>) -> AppResult<FirebaseApp> {
        api::get_app(name)
    }

    pub fn apps() -> Vec<FirebaseApp> {
        api::get_apps()
    }

    pub fn register_version(library: &str, version: &str, variant: Option<&str>) {
        api::register_version(library, version, variant)
    }

    pub fn set_log_level(level: LogLevel) {
        api::set_log_level(level)
    }

    pub fn on_log(callback: Option<LogCallback>, options: Option<LogOptions>) -> AppResult<()> {
        api::on_log(callback, options)
    }

    pub fn sdk_version() -> &'static str {
        api::SDK_VERSION
    }

    pub fn auth(app: Option<FirebaseApp>) -> AuthResult<Arc<Auth>> {
        auth::api::register_auth_component();
        let app = match app {
            Some(app) => app,
            None => api::get_app(None).map_err(AuthError::from)?,
        };
        auth::api::auth_for_app(app)
    }
}
