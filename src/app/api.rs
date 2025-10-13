use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use crate::app::component::{Component, ComponentContainer, ComponentType};
use crate::app::constants::{DEFAULT_ENTRY_NAME, PLATFORM_LOG_STRING};
use crate::app::errors::{AppError, AppResult};
use crate::app::logger::{self, LogCallback, LogLevel, LogOptions, LOGGER};
use crate::app::registry;
use crate::app::types::{
    deep_equal_config, deep_equal_options, get_default_app_config, FirebaseApp, FirebaseAppConfig,
    FirebaseAppSettings, FirebaseOptions, FirebaseServerApp, FirebaseServerAppSettings,
};

pub static SDK_VERSION: &str = env!("CARGO_PKG_VERSION");

static REGISTERED_VERSIONS: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn merged_settings(raw: Option<FirebaseAppSettings>) -> FirebaseAppSettings {
    raw.unwrap_or_default()
}

fn normalize_name(settings: &FirebaseAppSettings) -> AppResult<String> {
    let name = settings
        .name
        .clone()
        .unwrap_or_else(|| DEFAULT_ENTRY_NAME.to_string());
    if name.trim().is_empty() {
        return Err(AppError::BadAppName { app_name: name });
    }
    Ok(name)
}

fn automatic_data_collection(settings: &FirebaseAppSettings) -> bool {
    settings.automatic_data_collection_enabled.unwrap_or(true)
}

fn ensure_options(mut options: FirebaseOptions) -> AppResult<FirebaseOptions> {
    if !options_are_defined(&options) {
        if let Some(defaults) = get_default_app_config() {
            options = defaults;
        }
    }

    if !options_are_defined(&options) {
        return Err(AppError::NoOptions);
    }

    Ok(options)
}

fn options_are_defined(options: &FirebaseOptions) -> bool {
    options.api_key.is_some()
        || options.project_id.is_some()
        || options.app_id.is_some()
        || options.auth_domain.is_some()
        || options.database_url.is_some()
        || options.storage_bucket.is_some()
        || options.messaging_sender_id.is_some()
        || options.measurement_id.is_some()
}

pub fn initialize_app(
    options: FirebaseOptions,
    settings: Option<FirebaseAppSettings>,
) -> AppResult<FirebaseApp> {
    let settings = merged_settings(settings);
    let name = normalize_name(&settings)?;
    let automatic = automatic_data_collection(&settings);

    let options = ensure_options(options)?;

    let config = FirebaseAppConfig::new(name.clone(), automatic);

    {
        let apps = registry::apps().lock().unwrap();
        if let Some(existing) = apps.get(&name) {
            if deep_equal_options(&options, &existing.options())
                && deep_equal_config(&config, &existing.config())
            {
                return Ok(existing.clone());
            } else {
                return Err(AppError::DuplicateApp { app_name: name });
            }
        }
    }

    let container = ComponentContainer::new(name.clone());

    let components: Vec<Component> = {
        let global = registry::registered_components().lock().unwrap();
        global.values().cloned().collect()
    };

    let app = FirebaseApp::new(options.clone(), config.clone(), container.clone());

    use crate::component::types::{DynService, InstanceFactory};
    use std::sync::Arc;

    let app_factory: InstanceFactory =
        Arc::new(|_container, _options| Ok(Arc::new(()) as DynService));
    let _ = container.add_component(Component::new("app", app_factory, ComponentType::Public));
    for component in components {
        let _ = container.add_component(component);
    }

    registry::apps()
        .lock()
        .unwrap()
        .insert(name.clone(), app.clone());

    Ok(app)
}

pub fn get_app(name: Option<&str>) -> AppResult<FirebaseApp> {
    let lookup = name.unwrap_or(DEFAULT_ENTRY_NAME);
    if let Some(app) = registry::apps().lock().unwrap().get(lookup) {
        return Ok(app.clone());
    }
    Err(AppError::NoApp {
        app_name: lookup.to_string(),
    })
}

pub fn get_apps() -> Vec<FirebaseApp> {
    registry::apps().lock().unwrap().values().cloned().collect()
}

pub fn delete_app(app: &FirebaseApp) -> AppResult<()> {
    let name = app.name().to_string();
    let removed = registry::apps().lock().unwrap().remove(&name);

    if removed.is_some() {
        for provider in app.container().get_providers() {
            let _ = provider.delete();
        }
        app.set_is_deleted(true);
    }

    Ok(())
}

pub fn initialize_server_app(
    _options: Option<FirebaseOptions>,
    _settings: Option<FirebaseServerAppSettings>,
) -> AppResult<FirebaseServerApp> {
    Err(AppError::InvalidServerAppEnvironment)
}

pub fn register_version(library: &str, version: &str, variant: Option<&str>) {
    let mut library_key = PLATFORM_LOG_STRING
        .get(library)
        .copied()
        .unwrap_or(library)
        .to_string();
    if let Some(variant) = variant {
        library_key.push('-');
        library_key.push_str(variant);
    }

    if library_key.contains([' ', '/']) || version.contains([' ', '/']) {
        LOGGER.warn(format!(
            "Unable to register library '{library_key}' with version '{version}': contains illegal characters"
        ));
        return;
    }

    REGISTERED_VERSIONS
        .lock()
        .unwrap()
        .insert(library_key, version.to_string());
}

pub fn on_log(callback: Option<LogCallback>, options: Option<LogOptions>) -> AppResult<()> {
    logger::set_user_log_handler(callback, options);
    Ok(())
}

pub fn set_log_level(level: LogLevel) {
    let _ = logger::set_log_level(level);
}
