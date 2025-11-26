use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use serde_json::Value;

use crate::component::component::Component;
use crate::component::constants::DEFAULT_ENTRY_NAME;
use crate::component::container::{ComponentContainer, ComponentContainerInner};
use crate::component::types::{ComponentError, DynService, InstanceFactoryOptions, InstantiationMode};

#[derive(Clone)]
pub struct Provider {
    inner: Arc<ProviderInner>,
}

struct ProviderInner {
    name: Arc<str>,
    container: Weak<ComponentContainerInner>,
    component: Mutex<Option<Component>>,
    instances: Mutex<HashMap<Arc<str>, DynService>>,
    instance_options: Mutex<HashMap<Arc<str>, Value>>,
}

impl Provider {
    pub(crate) fn new(name: &str, container: ComponentContainer) -> Self {
        let inner = ProviderInner {
            name: Arc::from(name.to_owned()),
            container: Arc::downgrade(&container.inner),
            component: Mutex::new(None),
            instances: Mutex::new(HashMap::new()),
            instance_options: Mutex::new(HashMap::new()),
        };
        Self { inner: Arc::new(inner) }
    }

    pub fn name(&self) -> &str {
        &self.inner.name
    }

    pub fn component_type(&self) -> Option<crate::component::types::ComponentType> {
        self.inner
            .component
            .lock()
            .unwrap()
            .as_ref()
            .map(|component| component.component_type())
    }

    pub fn is_component_set(&self) -> bool {
        self.inner.component.lock().unwrap().is_some()
    }

    pub fn is_initialized(&self, identifier: Option<&str>) -> bool {
        let id = self.normalize_identifier(identifier);
        self.inner.instances.lock().unwrap().contains_key(&id)
    }

    pub fn clear_instance(&self, identifier: &str) {
        let id = Arc::from(identifier.to_owned());
        self.inner.instances.lock().unwrap().remove(&id);
        self.inner.instance_options.lock().unwrap().remove(&id);
    }

    pub fn delete(&self) -> Result<(), ComponentError> {
        self.inner.instances.lock().unwrap().clear();
        self.inner.instance_options.lock().unwrap().clear();
        Ok(())
    }

    pub fn get_immediate<T>(&self) -> Option<Arc<T>>
    where
        T: Any + Send + Sync + 'static,
    {
        self.get_immediate_with_options::<T>(None, true).ok().flatten()
    }

    pub fn get_immediate_with_options<T>(
        &self,
        identifier: Option<&str>,
        optional: bool,
    ) -> Result<Option<Arc<T>>, ComponentError>
    where
        T: Any + Send + Sync + 'static,
    {
        match self.get_or_initialize(identifier, Value::Null, false) {
            Ok(Some(service)) => match service.downcast::<T>() {
                Ok(value) => Ok(Some(value)),
                Err(_) => Ok(None),
            },
            Ok(None) => Ok(None),
            Err(err) => {
                if optional {
                    Ok(None)
                } else {
                    Err(err)
                }
            }
        }
    }

    pub fn initialize<T>(&self, options: Value, identifier: Option<&str>) -> Result<Arc<T>, ComponentError>
    where
        T: Any + Send + Sync + 'static,
    {
        if self.is_initialized(identifier) {
            return Err(ComponentError::InstanceAlreadyInitialized {
                name: self.name().to_string(),
                identifier: identifier.unwrap_or(DEFAULT_ENTRY_NAME).to_string(),
            });
        }

        match self.get_or_initialize(identifier, options.clone(), true)? {
            Some(service) => service
                .downcast::<T>()
                .map_err(|_| ComponentError::InstanceUnavailable {
                    name: self.name().to_string(),
                }),
            None => Err(ComponentError::InstanceUnavailable {
                name: self.name().to_string(),
            }),
        }
    }

    pub fn get_options(&self, identifier: Option<&str>) -> Value {
        let id = self.normalize_identifier(identifier);
        self.inner
            .instance_options
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .unwrap_or(Value::Null)
    }

    pub fn set_component(&self, component: Component) -> Result<(), ComponentError> {
        if component.name() != self.name() {
            return Err(ComponentError::MismatchingComponent {
                expected: self.name().to_string(),
                found: component.name().to_string(),
            });
        }

        {
            let mut guard = self.inner.component.lock().unwrap();
            if guard.is_some() {
                return Err(ComponentError::ComponentAlreadyProvided {
                    name: self.name().to_string(),
                });
            }
            *guard = Some(component.clone());
        }

        if !self.should_auto_initialize() {
            return Ok(());
        }

        if component.instantiation_mode() == InstantiationMode::Eager {
            let _ = self.get_or_initialize(Some(DEFAULT_ENTRY_NAME), Value::Null, true);
        }

        Ok(())
    }

    fn get_or_initialize(
        &self,
        identifier: Option<&str>,
        options: Value,
        force: bool,
    ) -> Result<Option<DynService>, ComponentError> {
        let id = self.normalize_identifier(identifier);

        if let Some(instance) = self.inner.instances.lock().unwrap().get(&id) {
            return Ok(Some(instance.clone()));
        }

        let component = match self.inner.component.lock().unwrap().clone() {
            Some(component) => component,
            None => return Ok(None),
        };

        if !force && !self.should_auto_initialize() {
            return Ok(None);
        }

        let container = match self.inner.container.upgrade() {
            Some(inner) => ComponentContainer { inner },
            None => {
                return Err(ComponentError::InitializationFailed {
                    name: self.name().to_string(),
                    reason: "container dropped".into(),
                });
            }
        };

        let options_record = options.clone();
        let factory_options = InstanceFactoryOptions::new(
            if id.as_ref() == DEFAULT_ENTRY_NAME {
                None
            } else {
                Some(id.to_string())
            },
            options,
        );

        let instance = match (component.instance_factory)(&container, factory_options) {
            Ok(instance) => instance,
            Err(err) => {
                return Err(ComponentError::InitializationFailed {
                    name: self.name().to_string(),
                    reason: err.to_string(),
                });
            }
        };

        self.inner
            .instances
            .lock()
            .unwrap()
            .insert(id.clone(), instance.clone());
        self.inner
            .instance_options
            .lock()
            .unwrap()
            .insert(id.clone(), options_record);

        if let Some(callback) = component.on_instance_created() {
            callback(&container, id.as_ref(), &instance);
        }

        Ok(Some(instance))
    }

    fn normalize_identifier(&self, identifier: Option<&str>) -> Arc<str> {
        let id = identifier.unwrap_or(DEFAULT_ENTRY_NAME);
        if let Some(component) = self.inner.component.lock().unwrap().as_ref() {
            if component.multiple_instances() {
                return Arc::from(id.to_owned());
            }
        }
        Arc::from(DEFAULT_ENTRY_NAME.to_owned())
    }

    fn should_auto_initialize(&self) -> bool {
        self.inner
            .component
            .lock()
            .unwrap()
            .as_ref()
            .map(|component| component.instantiation_mode() != InstantiationMode::Explicit)
            .unwrap_or(false)
    }
}
