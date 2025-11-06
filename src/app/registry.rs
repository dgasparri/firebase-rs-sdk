use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use crate::app::component::{self, Component, Provider};
use crate::component::constants::DEFAULT_ENTRY_NAME;
use crate::app::heartbeat::HeartbeatServiceImpl;
use crate::app::logger::LOGGER;
use crate::app::types::{FirebaseApp, FirebaseServerApp, HeartbeatService};
use crate::platform::runtime;

pub static APPS: LazyLock<Mutex<HashMap<String, FirebaseApp>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub static SERVER_APPS: LazyLock<Mutex<HashMap<String, FirebaseServerApp>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn apps_guard() -> MutexGuard<'static, HashMap<String, FirebaseApp>> {
    APPS.lock().unwrap_or_else(|poison| poison.into_inner())
}

pub(crate) fn server_apps_guard() -> MutexGuard<'static, HashMap<String, FirebaseServerApp>> {
    SERVER_APPS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

pub(crate) fn registered_components_guard() -> MutexGuard<'static, HashMap<Arc<str>, Component>> {
    component::global_components()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

/// Attaches a component to the given app, logging failures for debugging. 
/// Mirrors the JS `_addComponent` helper.
pub fn add_component(app: &FirebaseApp, component: &Component) {
    if app.container().add_component(component.clone()).is_err() {
        LOGGER.debug(format!(
            "Component {} failed to register with FirebaseApp {}",
            component.name(),
            app.name()
        ));
    }
}

/// Replaces any existing component with the same name on the given app.
/// Mirrors the JS `_addOrOverwriteComponent` helper.
#[allow(dead_code)]
pub fn add_or_overwrite_component(app: &FirebaseApp, component: Component) {
    app.container().add_or_overwrite_component(component);
}


/// Clears globally registered components
/// Mirrors the JS `_clearComponents` helper.
#[allow(dead_code)]
pub fn clear_components() {
    registered_components_guard().clear();
}


/// Registers a global component and propagates it to already-initialized apps.
pub fn register_component(component: Component) -> bool {
    if !component::register_component(component.clone()) {
        return false;
    }

    {
        let apps = apps_guard();
        for app in apps.values() {
            add_component(app, &component);
        }
    }

    {
        let server_apps = server_apps_guard();
        for server_app in server_apps.values() {
            add_component(server_app.base(), &component);
        }
    }

    true
}

/// Fetches the provider for the named component, triggering heartbeat side-effects.
/// Mirrors the JS `_getProvider` helper.
pub fn get_provider(app: &FirebaseApp, name: &str) -> Provider {
    let container = app.container();
    if let Some(service) = container
        .get_provider("heartbeat")
        .get_immediate::<HeartbeatServiceImpl>()
    {
        let app_name = app.name().to_string();
        let service_clone = service.clone();
        runtime::spawn_detached(async move {
            if let Err(err) = service_clone.trigger_heartbeat().await {
                LOGGER.debug(format!(
                    "Failed to trigger heartbeat for app {}: {}",
                    app_name, err
                ));
            }
        });
    }
    container.get_provider(name)
}


/// Removes a cached service instance from the given app by provider name.
/// Mirrors the JS `_removeServiceInstance` helper.
#[allow(dead_code)]
pub fn remove_service_instance(app: &FirebaseApp, name: &str, instance_identifier: Option<&str>) {
    let instance_identifier = instance_identifier.unwrap_or(DEFAULT_ENTRY_NAME);
    get_provider(app, name).clear_instance(instance_identifier);
}


/// Returns true when the supplied app corresponds to a server-side Firebase app instance.
#[allow(dead_code)]
pub fn is_firebase_server_app(app: &FirebaseApp) -> bool {
    server_apps_guard().contains_key(app.name())
}




#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::api;
    use crate::app::heartbeat::clear_heartbeat_store_for_tests;
    use crate::app::types::{FirebaseAppSettings, FirebaseOptions, FirebaseServerAppSettings};
    use crate::component::types::{ComponentType, DynService, InstanceFactory, InstantiationMode};
    use crate::component::Component;
    use crate::platform::runtime;
    use futures::lock::Mutex as AsyncMutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, LazyLock};
    use std::time::Duration;

    static TEST_GUARD: LazyLock<AsyncMutex<()>> = LazyLock::new(|| AsyncMutex::new(()));

    fn reset() {
        {
            let mut apps = apps_guard();
            for app in apps.values() {
                app.set_is_deleted(true);
            }
            apps.clear();
        }
        server_apps_guard().clear();
        registered_components_guard().clear();
        clear_heartbeat_store_for_tests();
        crate::component::global_components()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clear();
    }

    async fn with_serialized_test<F, Fut>(f: F) -> Fut::Output
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future,
    {
        let _guard = TEST_GUARD.lock().await;
        reset();
        f().await
    }

    fn test_options() -> FirebaseOptions {
        FirebaseOptions {
            api_key: Some("internal-test-key".into()),
            app_id: Some("1:987:web:test".into()),
            project_id: Some("internal-test".into()),
            ..Default::default()
        }
    }

    fn make_component(name: &str, factory: InstanceFactory) -> Component {
        Component::new(name.to_string(), factory, ComponentType::Public)
            .with_instantiation_mode(InstantiationMode::Lazy)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn add_component_attaches_to_app() {
        with_serialized_test(|| async {
            let app = api::initialize_app(test_options(), None)
                .await
                .expect("app init");
            let factory: InstanceFactory = Arc::new(|_, _| Ok(Arc::new(()) as DynService));
            let c = make_component("internal-comp", factory);
            add_component(&app, &c);

            assert!(app
                .container()
                .get_provider("internal-comp")
                .is_component_set());
        })
        .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn add_or_overwrite_component_replaces_existing_instance() {
        with_serialized_test(|| async {
            let app = api::initialize_app(test_options(), None)
                .await
                .expect("app init");

            let counter = Arc::new(AtomicUsize::new(0));
            let base_counter = counter.clone();
            let base_factory: InstanceFactory = Arc::new(move |_, _| {
                let value = base_counter.fetch_add(1, Ordering::SeqCst) + 1;
                Ok(Arc::new(value) as DynService)
            });
            add_component(&app, &make_component("overwrite", base_factory));

            let first_provider = app.container().get_provider("overwrite");
            let first = first_provider
                .get_immediate::<usize>()
                .expect("first instance")
                .as_ref()
                .clone();
            assert_eq!(first, 1);

            let counter_two = counter.clone();
            counter_two.store(40, Ordering::SeqCst);
            let replacement_factory: InstanceFactory = Arc::new(move |_, _| {
                let value = counter_two.fetch_add(1, Ordering::SeqCst) + 1;
                Ok(Arc::new(value) as DynService)
            });
            add_or_overwrite_component(&app, make_component("overwrite", replacement_factory));

            remove_service_instance(&app, "overwrite", None);
            let provider_after = app.container().get_provider("overwrite");
            let second = provider_after
                .get_immediate::<usize>()
                .expect("second instance")
                .as_ref()
                .clone();
            assert!(second > first);
        })
        .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn clear_components_drops_registry_entries() {
        with_serialized_test(|| async {
            let app = api::initialize_app(test_options(), None)
                .await
                .expect("app init");
            let factory: InstanceFactory = Arc::new(|_, _| Ok(Arc::new(()) as DynService));
            register_component(make_component("clearable", factory));
            assert!(registered_components_guard()
                .keys()
                .any(|name| name.as_ref() == "clearable"));

            clear_components();
            assert!(!registered_components_guard()
                .keys()
                .any(|name| name.as_ref() == "clearable"));
            assert!(app.container().get_provider("clearable").is_component_set());
        })
        .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn register_component_propagates_to_existing_apps() {
        with_serialized_test(|| async {
            let app = api::initialize_app(test_options(), None)
                .await
                .expect("app init");
            let factory: InstanceFactory = Arc::new(|_, _| Ok(Arc::new("shared") as DynService));

            register_component(make_component("late", factory));

            let provider = app.container().get_provider("late");
            assert!(provider.is_component_set());
        })
        .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn get_provider_and_remove_service_instance_reset_cached_instance() {
        with_serialized_test(|| async {
            let app = api::initialize_app(test_options(), None)
                .await
                .expect("app init");
            let counter = Arc::new(AtomicUsize::new(0));
            let counter_clone = counter.clone();
            let factory: InstanceFactory = Arc::new(move |_, _| {
                let value = counter_clone.fetch_add(1, Ordering::SeqCst) + 1;
                Ok(Arc::new(value) as DynService)
            });
            add_component(&app, &make_component("provider", factory));

            let provider = get_provider(&app, "provider");
            let first = provider
                .get_immediate::<usize>()
                .expect("first")
                .as_ref()
                .clone();
            assert_eq!(first, 1);

            remove_service_instance(&app, "provider", None);
            let second = provider
                .get_immediate::<usize>()
                .expect("second")
                .as_ref()
                .clone();
            assert_eq!(second, 2);
        })
        .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn is_firebase_server_app_detects_server_instances() {
        with_serialized_test(|| async {
            let server_settings = FirebaseServerAppSettings {
                automatic_data_collection_enabled: None,
                auth_id_token: None,
                app_check_token: None,
                release_on_deref: Some(true),
            };
            let server_app =
                api::initialize_server_app(Some(test_options()), Some(server_settings))
                    .await
                    .expect("server app");
            assert!(is_firebase_server_app(server_app.base()));

            drop(server_app);
            runtime::sleep(Duration::from_millis(25)).await;

            let app = api::initialize_app(
                test_options(),
                Some(FirebaseAppSettings {
                    name: Some("regular".into()),
                    automatic_data_collection_enabled: None,
                }),
            )
            .await
            .expect("regular app");
            assert!(!is_firebase_server_app(&app));
        })
        .await;
    }
}
