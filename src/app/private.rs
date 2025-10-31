use crate::app::registry;
use crate::app::types::FirebaseApp;
use crate::component::constants::DEFAULT_ENTRY_NAME;
use crate::component::Provider;

pub use crate::app::types::{
    AppHook, FirebaseAppInternals, FirebaseAuthTokenData, FirebaseServerApp, FirebaseService,
    FirebaseServiceFactory, FirebaseServiceInternals, FirebaseServiceNamespace,
    PlatformLoggerService, VersionService,
};

pub use crate::component::{Component, ComponentContainer};

/// Adds a component to the given app, mirroring the JS `_addComponent` helper.
pub fn add_component(app: &FirebaseApp, component: Component) {
    registry::add_component(app, &component);
}

/// Adds or overwrites a component on the given app (`_addOrOverwriteComponent`).
pub fn add_or_overwrite_component(app: &FirebaseApp, component: Component) {
    registry::add_or_overwrite_component(app, component);
}

/// Clears globally registered components (parity with `_clearComponents`).
pub fn clear_components() {
    registry::registered_components_guard().clear();
}

/// Fetches a provider from the given app and triggers heartbeat side effects (`_getProvider`).
pub fn get_provider(app: &FirebaseApp, name: &str) -> Provider {
    registry::get_provider(app, name)
}

/// Removes a cached service instance from the provider (`_removeServiceInstance`).
pub fn remove_service_instance(app: &FirebaseApp, name: &str, identifier: Option<&str>) {
    let id = identifier.unwrap_or(DEFAULT_ENTRY_NAME);
    registry::remove_service_instance(app, name, id);
}

/// Returns true when the supplied app corresponds to a server-side Firebase app instance.
pub fn is_firebase_server_app(app: &FirebaseApp) -> bool {
    registry::server_apps_guard().contains_key(app.name())
}

/// Registers a component globally so that future apps receive it.
pub fn register_component(component: Component) -> bool {
    registry::register_component(component)
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
            let mut apps = registry::apps_guard();
            for app in apps.values() {
                app.set_is_deleted(true);
            }
            apps.clear();
        }
        registry::server_apps_guard().clear();
        registry::registered_components_guard().clear();
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
            add_component(&app, make_component("internal-comp", factory));

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
            add_component(&app, make_component("overwrite", base_factory));

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
            assert!(registry::registered_components_guard()
                .keys()
                .any(|name| name.as_ref() == "clearable"));

            clear_components();
            assert!(!registry::registered_components_guard()
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
            add_component(&app, make_component("provider", factory));

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
