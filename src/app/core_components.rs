use std::sync::Arc;

use crate::app::api::{self, SDK_VERSION};
use crate::app::heartbeat::{HeartbeatServiceImpl, InMemoryHeartbeatStorage};
use crate::app::platform_logger::PlatformLoggerServiceImpl;
use crate::app::registry;
use crate::app::types::{FirebaseApp, HeartbeatStorage};
use crate::component::types::{
    ComponentError, ComponentType, DynService, InstanceFactory, InstantiationMode,
};
use crate::component::{Component, ComponentContainer};

use std::sync::LazyLock;

/// Ensures the core Firebase components are registered before app initialization.
pub fn ensure_registered() {
    LazyLock::force(&REGISTERED);
}

static REGISTERED: LazyLock<()> = LazyLock::new(|| {
    register_platform_logger_component();
    register_heartbeat_component();
    api::register_version("@firebase/app", SDK_VERSION, None);
    api::register_version("fire-js", "", None);
});

fn register_platform_logger_component() {
    let factory: InstanceFactory = Arc::new(|container: &ComponentContainer, _| {
        let service: DynService = Arc::new(PlatformLoggerServiceImpl::new(container.clone()));
        Ok(service)
    });

    let component = Component::new("platform-logger", factory, ComponentType::Private)
        .with_instantiation_mode(InstantiationMode::Eager);
    let _ = registry::register_component(component);
}

fn register_heartbeat_component() {
    let factory: InstanceFactory = Arc::new(|container: &ComponentContainer, _| {
        let app = container
            .get_provider("app")
            .get_immediate::<FirebaseApp>()
            .ok_or_else(|| ComponentError::InitializationFailed {
                name: "heartbeat".to_string(),
                reason: "App provider unavailable".to_string(),
            })?;
        let app = (*app).clone();
        let storage: Arc<dyn HeartbeatStorage> = Arc::new(InMemoryHeartbeatStorage::new(&app));
        let service: DynService = Arc::new(HeartbeatServiceImpl::new(app, storage));
        Ok(service)
    });

    let component = Component::new("heartbeat", factory, ComponentType::Private)
        .with_instantiation_mode(InstantiationMode::Lazy);
    let _ = registry::register_component(component);
}
