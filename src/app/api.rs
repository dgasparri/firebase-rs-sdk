use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use crate::app::component::{Component, ComponentContainer, ComponentType};
use crate::app::constants::{DEFAULT_ENTRY_NAME, PLATFORM_LOG_STRING};
use crate::app::ensure_core_components_registered;
use crate::app::errors::{AppError, AppResult};
use crate::app::logger::{self, LogCallback, LogLevel, LogOptions, LOGGER};
use crate::app::registry::{self, apps_guard, registered_components_guard, server_apps_guard};
use crate::app::types::{
    deep_equal_config, deep_equal_options, get_default_app_config, FirebaseApp, FirebaseAppConfig,
    FirebaseAppSettings, FirebaseOptions, FirebaseServerApp, FirebaseServerAppSettings,
    VersionService,
};
use crate::app::types::{is_browser, is_web_worker};
use crate::component::types::{DynService, InstanceFactory, InstantiationMode};
use sha2::{Digest, Sha256};

pub static SDK_VERSION: &str = env!("CARGO_PKG_VERSION");

static REGISTERED_VERSIONS: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static GLOBAL_APP_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn global_app_guard() -> MutexGuard<'static, ()> {
    GLOBAL_APP_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

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

fn validate_token_ttl(token: Option<&str>, token_name: &str) {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    let Some(token) = token else {
        return;
    };

    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 {
        LOGGER.warn(format!(
            "FirebaseServerApp {token_name} is invalid: second part could not be parsed."
        ));
        return;
    }

    let Ok(decoded) = STANDARD.decode(parts[1]) else {
        LOGGER.warn(format!(
            "FirebaseServerApp {token_name} is invalid: second part could not be parsed."
        ));
        return;
    };

    let Ok(claims) = serde_json::from_slice::<serde_json::Value>(&decoded) else {
        LOGGER.warn(format!(
            "FirebaseServerApp {token_name} is invalid: expiration claim could not be parsed"
        ));
        return;
    };

    let exp_ms = claims
        .get("exp")
        .and_then(|value| value.as_i64())
        .map(|seconds| seconds * 1000);

    let Some(exp) = exp_ms else {
        LOGGER.warn(format!(
            "FirebaseServerApp {token_name} is invalid: expiration claim could not be parsed"
        ));
        return;
    };

    let now = chrono::Utc::now().timestamp_millis();
    if exp <= now {
        LOGGER.warn(format!(
            "FirebaseServerApp {token_name} is invalid: the token has expired."
        ));
    }
}

fn supports_finalization_registry() -> bool {
    false
}

fn server_app_hash(options: &FirebaseOptions, settings: &FirebaseServerAppSettings) -> String {
    let mut hasher = Sha256::new();

    fn write_option(hasher: &mut Sha256, value: &Option<String>) {
        if let Some(v) = value {
            hasher.update(v.as_bytes());
        }
        hasher.update([0]);
    }

    write_option(&mut hasher, &options.api_key);
    write_option(&mut hasher, &options.auth_domain);
    write_option(&mut hasher, &options.database_url);
    write_option(&mut hasher, &options.project_id);
    write_option(&mut hasher, &options.storage_bucket);
    write_option(&mut hasher, &options.messaging_sender_id);
    write_option(&mut hasher, &options.app_id);
    write_option(&mut hasher, &options.measurement_id);

    write_option(
        &mut hasher,
        &settings
            .automatic_data_collection_enabled
            .map(|value| value.to_string()),
    );
    write_option(&mut hasher, &settings.auth_id_token);
    write_option(&mut hasher, &settings.app_check_token);

    let digest = hasher.finalize();
    format!("serverapp-{digest:x}")
}

/// Creates (or returns) a `FirebaseApp` instance for the provided options and settings.
///
/// When an app with the same normalized name already exists, the existing instance is
/// returned as long as the configuration matches. A mismatch results in `AppError::DuplicateApp`.
pub async fn initialize_app(
    options: FirebaseOptions,
    settings: Option<FirebaseAppSettings>,
) -> AppResult<FirebaseApp> {
    ensure_core_components_registered();
    let _guard = global_app_guard();
    let settings = merged_settings(settings);
    let name = normalize_name(&settings)?;
    let automatic = automatic_data_collection(&settings);

    let options = ensure_options(options)?;

    let config = FirebaseAppConfig::new(name.clone(), automatic);

    {
        let apps = apps_guard();
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
        let global = registered_components_guard();
        global.values().cloned().collect()
    };

    let app = FirebaseApp::new(options.clone(), config.clone(), container.clone());

    let app_for_factory = app.clone();
    let app_factory: InstanceFactory =
        Arc::new(move |_container, _options| Ok(Arc::new(app_for_factory.clone()) as DynService));
    let _ = container.add_component(Component::new("app", app_factory, ComponentType::Public));
    for component in components {
        let _ = container.add_component(component);
    }

    apps_guard().insert(name.clone(), app.clone());

    Ok(app)
}

/// Retrieves a previously initialized `FirebaseApp` by name.
///
/// Passing `None` looks up the default app entry.
pub async fn get_app(name: Option<&str>) -> AppResult<FirebaseApp> {
    ensure_core_components_registered();
    let _guard = global_app_guard();
    let lookup = name.unwrap_or(DEFAULT_ENTRY_NAME);
    if let Some(app) = apps_guard().get(lookup) {
        return Ok(app.clone());
    }
    Err(AppError::NoApp {
        app_name: lookup.to_string(),
    })
}

/// Returns a snapshot of all registered `FirebaseApp` instances.
pub async fn get_apps() -> Vec<FirebaseApp> {
    ensure_core_components_registered();
    let _guard = global_app_guard();
    apps_guard().values().cloned().collect()
}

/// Deletes the provided `FirebaseApp` from the global registry and tears down services.
pub async fn delete_app(app: &FirebaseApp) -> AppResult<()> {
    let _guard = global_app_guard();
    let name = app.name().to_string();
    let removed = apps_guard().remove(&name);

    if removed.is_some() {
        for provider in app.container().get_providers() {
            let _ = provider.delete();
        }
        app.set_is_deleted(true);
    }

    Ok(())
}

/// Creates or reuses a server-side `FirebaseServerApp` instance from options and settings.
pub async fn initialize_server_app(
    options: Option<FirebaseOptions>,
    settings: Option<FirebaseServerAppSettings>,
) -> AppResult<FirebaseServerApp> {
    ensure_core_components_registered();

    if is_browser() && !is_web_worker() {
        return Err(AppError::InvalidServerAppEnvironment);
    }

    let mut server_settings = settings.unwrap_or_default();
    if server_settings.automatic_data_collection_enabled.is_none() {
        server_settings.automatic_data_collection_enabled = Some(true);
    }

    let app_options = match options.or_else(get_default_app_config) {
        Some(opts) => opts,
        None => return Err(AppError::NoOptions),
    };

    validate_token_ttl(server_settings.auth_id_token.as_deref(), "authIdToken");
    validate_token_ttl(server_settings.app_check_token.as_deref(), "appCheckToken");

    if server_settings.release_on_deref.is_some() && !supports_finalization_registry() {
        return Err(AppError::FinalizationRegistryNotSupported);
    }

    let name = server_app_hash(&app_options, &server_settings);

    let container = ComponentContainer::new(name.clone());
    for component in registered_components_guard().values() {
        let _ = container.add_component(component.clone());
    }

    let base_app = FirebaseApp::new(
        app_options.clone(),
        FirebaseAppConfig::new(
            name.clone(),
            server_settings
                .automatic_data_collection_enabled
                .unwrap_or(true),
        ),
        container.clone(),
    );

    let base_for_factory = base_app.clone();
    let app_factory: InstanceFactory =
        Arc::new(move |_container, _| Ok(Arc::new(base_for_factory.clone()) as DynService));
    let _ = container.add_component(Component::new("app", app_factory, ComponentType::Public));

    let server_app = FirebaseServerApp::new(base_app, server_settings.clone());

    {
        let _guard = global_app_guard();
        if let Some(existing) = server_apps_guard().get(&name) {
            existing.inc_ref_count();
            return Ok(existing.clone());
        }

        server_apps_guard().insert(name.clone(), server_app.clone());
    }

    register_version("@firebase/app", SDK_VERSION, Some("serverapp")).await;

    Ok(server_app)
}

/// Registers a library version component so it can be queried by other Firebase services.
pub async fn register_version(library: &str, version: &str, variant: Option<&str>) {
    let _guard = global_app_guard();
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
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(library_key.clone(), version.to_string());

    let component_name = format!("{library_key}-version");
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
    REGISTERED_VERSIONS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clear();
}

/// Installs a user-supplied logger that receives Firebase diagnostic messages.
pub fn on_log(callback: Option<LogCallback>, options: Option<LogOptions>) -> AppResult<()> {
    logger::set_user_log_handler(callback, options);
    Ok(())
}

/// Sets the global Firebase SDK log level.
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
    use futures::executor::block_on;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, LazyLock, Mutex};

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);
    static TEST_SERIAL: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

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
        {
            let _guard = super::global_app_guard();
            let mut apps = registry::apps_guard();
            for app in apps.values() {
                app.set_is_deleted(true);
            }
            apps.clear();
            registry::server_apps_guard().clear();
        }

        assert!(registry::apps_guard().is_empty());
        crate::component::clear_global_components_for_test();
        clear_registered_versions_for_tests();
        clear_heartbeat_store_for_tests();
    }

    fn with_serialized_test<F: FnOnce()>(f: F) {
        let _guard = TEST_SERIAL.lock().unwrap();
        reset();
        f();
    }

    fn make_test_component(name: &str) -> Component {
        let factory: InstanceFactory = Arc::new(|_, _| Ok(Arc::new(()) as DynService));
        Component::new(name.to_string(), factory, ComponentType::Public)
            .with_instantiation_mode(InstantiationMode::Lazy)
    }

    #[test]
    fn initialize_app_creates_default_app() {
        with_serialized_test(|| {
            let app = block_on(super::initialize_app(test_options(), None)).expect("init app");
            assert_eq!(app.name(), DEFAULT_ENTRY_NAME);
        });
    }

    #[test]
    fn initialize_app_creates_named_app() {
        with_serialized_test(|| {
            let app = block_on(super::initialize_app(
                test_options(),
                Some(FirebaseAppSettings {
                    name: Some("MyApp".to_string()),
                    automatic_data_collection_enabled: None,
                }),
            ))
            .expect("init named app");
            assert_eq!(app.name(), "MyApp");
        });
    }

    #[test]
    fn initialize_app_with_same_options_returns_same_instance() {
        with_serialized_test(|| {
            let opts = test_options();
            let app1 = block_on(super::initialize_app(opts.clone(), None)).expect("first init");
            let app2 = block_on(super::initialize_app(opts, None)).expect("second init");
            let container1 = app1.container().inner.clone();
            let container2 = app2.container().inner.clone();
            assert!(Arc::ptr_eq(&container1, &container2));
        });
    }

    #[test]
    fn initialize_app_duplicate_options_fails() {
        with_serialized_test(|| {
            let app_name = next_name("dup-app");
            let opts1 = test_options();
            let settings = FirebaseAppSettings {
                name: Some(app_name.clone()),
                automatic_data_collection_enabled: None,
            };
            let _ = block_on(super::initialize_app(opts1.clone(), Some(settings.clone())))
                .expect("first init");
            let mut opts2 = opts1.clone();
            opts2.api_key = Some("other-key".to_string());
            let result = block_on(super::initialize_app(opts2, Some(settings)));
            assert!(matches!(result, Err(AppError::DuplicateApp { .. })));
        });
    }

    #[test]
    fn initialize_app_duplicate_config_fails() {
        with_serialized_test(|| {
            let opts = test_options();
            let settings = FirebaseAppSettings {
                name: Some("dup".to_string()),
                automatic_data_collection_enabled: Some(true),
            };
            let _ = block_on(super::initialize_app(opts.clone(), Some(settings.clone())))
                .expect("first init");
            let mut other = settings.clone();
            other.automatic_data_collection_enabled = Some(false);
            let result = block_on(super::initialize_app(opts, Some(other)));
            assert!(matches!(result, Err(AppError::DuplicateApp { .. })));
        });
    }

    #[test]
    fn automatic_data_collection_defaults_true() {
        with_serialized_test(|| {
            let app = block_on(super::initialize_app(test_options(), None)).expect("init app");
            assert!(app.automatic_data_collection_enabled());
        });
    }

    #[test]
    fn automatic_data_collection_respects_setting() {
        with_serialized_test(|| {
            let app = block_on(super::initialize_app(
                test_options(),
                Some(FirebaseAppSettings {
                    name: None,
                    automatic_data_collection_enabled: Some(false),
                }),
            ))
            .expect("init app");
            assert!(!app.automatic_data_collection_enabled());
        });
    }

    #[test]
    fn registered_components_attach_to_new_app() {
        with_serialized_test(|| {
            let name1 = next_name("test-component");
            let name2 = next_name("test-component");
            let _ = registry::register_component(make_test_component(&name1));
            let _ = registry::register_component(make_test_component(&name2));

            let app = block_on(super::initialize_app(test_options(), None)).expect("init app");
            assert!(app.container().get_provider(&name1).is_component_set());
            assert!(app.container().get_provider(&name2).is_component_set());
        });
    }

    #[test]
    fn delete_app_marks_app_deleted_and_clears_registry() {
        with_serialized_test(|| {
            let app = block_on(super::initialize_app(test_options(), None)).expect("init app");
            let name = app.name().to_string();
            {
                let apps = registry::apps_guard();
                assert!(apps.contains_key(&name));
            }
            assert!(block_on(super::delete_app(&app)).is_ok());
            assert!(app.is_deleted());
            {
                let apps = registry::apps_guard();
                assert!(!apps.contains_key(&name));
            }
        });
    }

    #[test]
    fn register_version_registers_component() {
        with_serialized_test(|| {
            let library = next_name("lib");
            block_on(super::register_version(&library, "1.0.0", None));
            let components = registry::registered_components_guard();
            let expected = format!("{}-version", library);
            assert!(components.keys().any(|key| key.as_ref() == expected));
        });
    }

    #[test]
    fn get_app_returns_existing_app() {
        with_serialized_test(|| {
            let created = block_on(super::initialize_app(test_options(), None)).expect("init app");
            let fetched = block_on(super::get_app(None)).expect("get app");
            assert_eq!(created.name(), fetched.name());
        });
    }

    #[test]
    fn get_app_nonexistent_fails() {
        with_serialized_test(|| {
            let result = block_on(super::get_app(Some("missing")));
            assert!(matches!(result, Err(AppError::NoApp { .. })));
        });
    }
}
