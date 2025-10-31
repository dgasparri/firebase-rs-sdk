use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use crate::app::component::{self, Component, Provider};
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
pub fn add_component(app: &FirebaseApp, component: &Component) {
    if app.container().add_component(component.clone()).is_err() {
        LOGGER.debug(format!(
            "Component {} failed to register with FirebaseApp {}",
            component.name(),
            app.name()
        ));
    }
}

#[allow(dead_code)]
/// Replaces any existing component with the same name on the given app.
pub fn add_or_overwrite_component(app: &FirebaseApp, component: Component) {
    app.container().add_or_overwrite_component(component);
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

#[allow(dead_code)]
/// Deletes a specific service instance from the given app by provider name.
pub fn remove_service_instance(app: &FirebaseApp, name: &str, instance_identifier: &str) {
    get_provider(app, name).clear_instance(instance_identifier);
}
