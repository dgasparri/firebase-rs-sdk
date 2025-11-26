use crate::app::FirebaseApp;
use crate::installations::error::{invalid_argument, InstallationsResult};

/// Extracted configuration required to contact the Firebase Installations REST API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppConfig {
    pub app_name: String,
    pub api_key: String,
    pub project_id: String,
    pub app_id: String,
}

/// Mirrors the JavaScript helper `extractAppConfig` from
/// `packages/installations/src/helpers/extract-app-config.ts`.
pub fn extract_app_config(app: &FirebaseApp) -> InstallationsResult<AppConfig> {
    let options = app.options();

    let app_name = app.name().to_owned();
    if app_name.is_empty() {
        return Err(missing_value_error("App Name"));
    }

    let project_id = options.project_id.ok_or_else(|| missing_value_error("projectId"))?;
    let api_key = options.api_key.ok_or_else(|| missing_value_error("apiKey"))?;
    let app_id = options.app_id.ok_or_else(|| missing_value_error("appId"))?;

    Ok(AppConfig {
        app_name,
        api_key,
        project_id,
        app_id,
    })
}

fn missing_value_error(value_name: &str) -> crate::installations::error::InstallationsError {
    invalid_argument(format!("Missing App configuration value: \"{}\"", value_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn base_options() -> FirebaseOptions {
        FirebaseOptions {
            project_id: Some("project".into()),
            api_key: Some("apikey".into()),
            app_id: Some("app-id".into()),
            ..Default::default()
        }
    }

    fn unique_settings() -> FirebaseAppSettings {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!("config-test-{}", COUNTER.fetch_add(1, Ordering::SeqCst))),
            ..Default::default()
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extract_app_config_success() {
        let options = base_options();
        let app = initialize_app(options, Some(unique_settings())).await.unwrap();
        let config = extract_app_config(&app).unwrap();
        assert_eq!(config.project_id, "project");
        assert_eq!(config.api_key, "apikey");
        assert_eq!(config.app_id, "app-id");
        assert_eq!(config.app_name, app.name());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn missing_project_id_returns_error() {
        let mut options = base_options();
        options.project_id = None;
        let app = initialize_app(options, Some(unique_settings())).await.unwrap();
        let err = extract_app_config(&app).unwrap_err();
        assert!(err.to_string().contains("Missing App configuration value"));
    }
}
