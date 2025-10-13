use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use crate::app::component::{self, Component, Provider};
use crate::app::heartbeat::HeartbeatServiceImpl;
use crate::app::logger::LOGGER;
use crate::app::types::{FirebaseApp, FirebaseServerApp, HeartbeatService};

pub static APPS: LazyLock<Mutex<HashMap<String, FirebaseApp>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub static SERVER_APPS: LazyLock<Mutex<HashMap<String, FirebaseServerApp>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn apps() -> &'static Mutex<HashMap<String, FirebaseApp>> {
    &APPS
}

pub fn server_apps() -> &'static Mutex<HashMap<String, FirebaseServerApp>> {
    &SERVER_APPS
}

pub fn registered_components() -> &'static Mutex<HashMap<Arc<str>, Component>> {
    component::global_components()
}

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
pub fn add_or_overwrite_component(app: &FirebaseApp, component: Component) {
    app.container().add_or_overwrite_component(component);
}

pub fn register_component(component: Component) -> bool {
    if !component::register_component(component.clone()) {
        return false;
    }

    for app in apps().lock().unwrap().values() {
        add_component(app, &component);
    }

    for server_app in server_apps().lock().unwrap().values() {
        add_component(server_app.base(), &component);
    }

    true
}

pub fn get_provider(app: &FirebaseApp, name: &str) -> Provider {
    let container = app.container();
    if let Some(service) = container
        .get_provider("heartbeat")
        .get_immediate::<HeartbeatServiceImpl>()
    {
        if let Err(err) = service.trigger_heartbeat() {
            LOGGER.debug(format!(
                "Failed to trigger heartbeat for app {}: {}",
                app.name(),
                err
            ));
        }
    }
    container.get_provider(name)
}

#[allow(dead_code)]
pub fn remove_service_instance(app: &FirebaseApp, name: &str, instance_identifier: &str) {
    get_provider(app, name).clear_instance(instance_identifier);
}
