use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::component::provider::Provider;
use crate::component::types::{ComponentError, DynService};
use crate::component::Component;

#[derive(Clone)]
pub struct ComponentContainer {
    pub(crate) inner: Arc<ComponentContainerInner>,
}

pub(crate) struct ComponentContainerInner {
    pub name: Arc<str>,
    pub providers: Mutex<HashMap<Arc<str>, Provider>>, // Provider holds Arc to inner state
    pub root_service: Mutex<Option<DynService>>,
}

impl ComponentContainer {
    pub fn new(name: impl Into<String>) -> Self {
        let name: Arc<str> = Arc::from(name.into());
        Self {
            inner: Arc::new(ComponentContainerInner {
                name,
                providers: Mutex::new(HashMap::new()),
                root_service: Mutex::new(None),
            }),
        }
    }

    pub fn name(&self) -> &str {
        &self.inner.name
    }

    pub fn add_component(&self, component: Component) -> Result<(), ComponentError> {
        let provider = self.get_provider(component.name());
        provider.set_component(component)
    }

    pub fn add_or_overwrite_component(&self, component: Component) {
        {
            let mut guard = self.inner.providers.lock().unwrap();
            if guard.contains_key(component.name()) {
                guard.remove(component.name());
            }
        }
        let _ = self.add_component(component);
    }

    pub fn get_provider(&self, name: &str) -> Provider {
        if let Some(provider) = self.inner.providers.lock().unwrap().get(name) {
            return provider.clone();
        }

        let provider = Provider::new(name, self.clone());
        self.inner
            .providers
            .lock()
            .unwrap()
            .insert(Arc::from(name.to_owned()), provider.clone());
        provider
    }

    pub fn get_providers(&self) -> Vec<Provider> {
        self.inner
            .providers
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect()
    }

    pub fn attach_root_service(&self, service: DynService) {
        *self.inner.root_service.lock().unwrap() = Some(service);
    }

    pub fn root_service<T: 'static + Send + Sync>(&self) -> Option<Arc<T>> {
        self.inner
            .root_service
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|svc| Arc::clone(svc).downcast::<T>().ok())
    }
}
