use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use crate::app::component::{Component, ComponentContainer, ComponentType};
use crate::app::constants::{DEFAULT_ENTRY_NAME, PLATFORM_LOG_STRING};
use crate::app::ensure_core_components_registered;
use crate::app::errors::{AppError, AppResult};
use crate::app::logger::{self, LogCallback, LogLevel, LogOptions, LOGGER};
use crate::app::registry;
use crate::app::types::{
    deep_equal_config, deep_equal_options, get_default_app_config, FirebaseApp, FirebaseAppConfig,
    FirebaseAppSettings, FirebaseOptions, FirebaseServerApp, FirebaseServerAppSettings,
    VersionService,
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
    ensure_core_components_registered();
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

    let app_for_factory = app.clone();
    let app_factory: InstanceFactory =
        Arc::new(move |_container, _options| Ok(Arc::new(app_for_factory.clone()) as DynService));
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
    ensure_core_components_registered();
    let lookup = name.unwrap_or(DEFAULT_ENTRY_NAME);
    if let Some(app) = registry::apps().lock().unwrap().get(lookup) {
        return Ok(app.clone());
    }
    Err(AppError::NoApp {
        app_name: lookup.to_string(),
    })
}

pub fn get_apps() -> Vec<FirebaseApp> {
    ensure_core_components_registered();
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
        .insert(library_key.clone(), version.to_string());

    use crate::component::types::{DynService, InstanceFactory, InstantiationMode};
    use crate::component::Component;
    use std::sync::Arc;

    let component_name = format!("{}-version", library_key);
    let version_string = version.to_string();
    let library_string = library_key.clone();
    let factory: InstanceFactory = Arc::new(move |_, _| {
        let service = VersionService {
            library: library_string.clone(),
            version: version_string.clone(),
        };
        Ok(Arc::new(service) as DynService)
    });

    let component = Component::new(component_name, factory, ComponentType::Version)
        .with_instantiation_mode(InstantiationMode::Eager);
    let _ = registry::register_component(component);
}

#[cfg(test)]
pub(crate) fn clear_registered_versions_for_tests() {
    REGISTERED_VERSIONS.lock().unwrap().clear();
}

pub fn on_log(callback: Option<LogCallback>, options: Option<LogOptions>) -> AppResult<()> {
    logger::set_user_log_handler(callback, options);
    Ok(())
}

pub fn set_log_level(level: LogLevel) {
    let _ = logger::set_log_level(level);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::heartbeat::clear_heartbeat_store_for_tests;
    use crate::app::registry;
    use crate::component::types::{ComponentType, DynService, InstanceFactory, InstantiationMode};
    use crate::component::Component;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn next_name(prefix: &str) -> String {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("{}-{}", prefix, id)
    }

    fn test_options() -> FirebaseOptions {
        FirebaseOptions {
            api_key: Some("test-key".to_string()),
            project_id: Some("test-project".to_string()),
            app_id: Some("1:123:web:test".to_string()),
            ..Default::default()
        }
    }

    fn reset() {
        for app in super::get_apps() {
            let _ = super::delete_app(&app);
        }
        registry::apps().lock().unwrap().clear();
        registry::server_apps().lock().unwrap().clear();
        clear_registered_versions_for_tests();
        clear_heartbeat_store_for_tests();
    }

    fn make_test_component(name: &str) -> Component {
        let factory: InstanceFactory = Arc::new(|_, _| Ok(Arc::new(()) as DynService));
        Component::new(name.to_string(), factory, ComponentType::Public)
            .with_instantiation_mode(InstantiationMode::Lazy)
    }

    #[test]
    fn initialize_app_creates_default_app() {
        reset();
        let app = super::initialize_app(test_options(), None).expect("init app");
        assert_eq!(app.name(), DEFAULT_ENTRY_NAME);
    }

    #[test]
    fn initialize_app_creates_named_app() {
        reset();
        let app = super::initialize_app(
            test_options(),
            Some(FirebaseAppSettings {
                name: Some("MyApp".to_string()),
                automatic_data_collection_enabled: None,
            }),
        )
        .expect("init named app");
        assert_eq!(app.name(), "MyApp");
    }

    #[test]
    fn initialize_app_with_same_options_returns_same_instance() {
        reset();
        let opts = test_options();
        let app1 = super::initialize_app(opts.clone(), None).expect("first init");
        let app2 = super::initialize_app(opts, None).expect("second init");
        let container1 = app1.container().inner.clone();
        let container2 = app2.container().inner.clone();
        assert!(Arc::ptr_eq(&container1, &container2));
    }

    #[test]
    fn initialize_app_duplicate_options_fails() {
        reset();
        let app_name = next_name("dup-app");
        let opts1 = test_options();
        let settings = FirebaseAppSettings {
            name: Some(app_name.clone()),
            automatic_data_collection_enabled: None,
        };
        let _ = super::initialize_app(opts1.clone(), Some(settings.clone())).expect("first init");
        let mut opts2 = opts1.clone();
        opts2.api_key = Some("other-key".to_string());
        let result = super::initialize_app(opts2, Some(settings));
        assert!(matches!(result, Err(AppError::DuplicateApp { .. })));
    }

    #[test]
    fn initialize_app_duplicate_config_fails() {
        reset();
        let opts = test_options();
        let settings = FirebaseAppSettings {
            name: Some("dup".to_string()),
            automatic_data_collection_enabled: Some(true),
        };
        let _ = super::initialize_app(opts.clone(), Some(settings.clone())).expect("first init");
        let mut other = settings.clone();
        other.automatic_data_collection_enabled = Some(false);
        let result = super::initialize_app(opts, Some(other));
        assert!(matches!(result, Err(AppError::DuplicateApp { .. })));
    }

    #[test]
    fn automatic_data_collection_defaults_true() {
        reset();
        let app = super::initialize_app(test_options(), None).expect("init app");
        assert!(app.automatic_data_collection_enabled());
    }

    #[test]
    fn automatic_data_collection_respects_setting() {
        reset();
        let app = super::initialize_app(
            test_options(),
            Some(FirebaseAppSettings {
                name: None,
                automatic_data_collection_enabled: Some(false),
            }),
        )
        .expect("init app");
        assert!(!app.automatic_data_collection_enabled());
    }

    #[test]
    fn registered_components_attach_to_new_app() {
        reset();
        let name1 = next_name("test-component");
        let name2 = next_name("test-component");
        let _ = registry::register_component(make_test_component(&name1));
        let _ = registry::register_component(make_test_component(&name2));

        let app = super::initialize_app(test_options(), None).expect("init app");
        assert!(app.container().get_provider(&name1).is_component_set());
        assert!(app.container().get_provider(&name2).is_component_set());
    }

    #[test]
    fn delete_app_marks_app_deleted_and_clears_registry() {
        reset();
        let app = super::initialize_app(test_options(), None).expect("init app");
        let name = app.name().to_string();
        {
            let apps = registry::apps().lock().unwrap();
            assert!(apps.contains_key(&name));
        }
        assert!(super::delete_app(&app).is_ok());
        assert!(app.is_deleted());
        {
            let apps = registry::apps().lock().unwrap();
            assert!(!apps.contains_key(&name));
        }
    }

    #[test]
    fn register_version_registers_component() {
        reset();
        let library = next_name("lib");
        super::register_version(&library, "1.0.0", None);
        let components = registry::registered_components().lock().unwrap();
        let expected = format!("{}-version", library);
        assert!(components.keys().any(|key| key.as_ref() == expected));
    }

    #[test]
    fn get_app_returns_existing_app() {
        reset();
        let created = super::initialize_app(test_options(), None).expect("init app");
        let fetched = super::get_app(None).expect("get app");
        assert_eq!(created.name(), fetched.name());
    }

    #[test]
    fn get_app_nonexistent_fails() {
        reset();
        let result = super::get_app(Some("missing"));
        assert!(matches!(result, Err(AppError::NoApp { .. })));
    }
}
