mod component;
pub mod constants;
pub mod container;
pub mod provider;
pub mod types;

pub use component::Component;
pub use constants::DEFAULT_ENTRY_NAME;
pub use container::ComponentContainer;
pub use provider::Provider;
pub use types::{
    ComponentError, ComponentType, InstanceFactory, InstanceFactoryOptions, InstantiationMode,
};

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

#[cfg(test)]
mod tests;

static GLOBAL_COMPONENTS: LazyLock<Mutex<HashMap<Arc<str>, Component>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn global_components() -> &'static Mutex<HashMap<Arc<str>, Component>> {
    &GLOBAL_COMPONENTS
}

pub fn register_component(component: Component) -> bool {
    let mut guard = GLOBAL_COMPONENTS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if guard.contains_key(component.name()) {
        return false;
    }
    guard.insert(Arc::from(component.name().to_owned()), component);
    true
}

#[cfg(test)]
pub fn clear_global_components_for_test() {
    GLOBAL_COMPONENTS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clear();
}
