use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{Map, Value};

use crate::component::types::{
    ComponentType, InstanceFactory, InstantiationMode, OnInstanceCreatedCallback,
};

#[derive(Clone)]
pub struct Component {
    name: Arc<str>,
    pub(crate) instance_factory: InstanceFactory,
    pub(crate) ty: ComponentType,
    pub(crate) instantiation_mode: InstantiationMode,
    pub(crate) multiple_instances: bool,
    pub(crate) service_props: Map<String, Value>,
    pub(crate) on_instance_created: Option<OnInstanceCreatedCallback>,
}

impl Component {
    pub fn new(
        name: impl Into<String>,
        instance_factory: InstanceFactory,
        ty: ComponentType,
    ) -> Self {
        Self {
            name: Arc::from(name.into()),
            instance_factory,
            ty,
            instantiation_mode: InstantiationMode::Lazy,
            multiple_instances: false,
            service_props: Map::new(),
            on_instance_created: None,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn component_type(&self) -> ComponentType {
        self.ty
    }

    pub fn instantiation_mode(&self) -> InstantiationMode {
        self.instantiation_mode
    }

    pub fn multiple_instances(&self) -> bool {
        self.multiple_instances
    }

    pub fn service_props(&self) -> &Map<String, Value> {
        &self.service_props
    }

    pub fn on_instance_created(&self) -> Option<&OnInstanceCreatedCallback> {
        self.on_instance_created.as_ref()
    }

    pub fn with_instantiation_mode(mut self, mode: InstantiationMode) -> Self {
        self.instantiation_mode = mode;
        self
    }

    pub fn with_multiple_instances(mut self, multiple: bool) -> Self {
        self.multiple_instances = multiple;
        self
    }

    pub fn with_service_props(mut self, props: HashMap<String, Value>) -> Self {
        self.service_props = props.into_iter().collect();
        self
    }

    pub fn with_instance_created_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(
                &crate::component::container::ComponentContainer,
                &str,
                &crate::component::types::DynService,
            ) + Send
            + Sync
            + 'static,
    {
        self.on_instance_created = Some(Arc::new(callback));
        self
    }
}
