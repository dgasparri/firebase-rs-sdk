use serde_json::Value;
use std::any::Any;
use std::fmt;
use std::sync::Arc;

use crate::component::container::ComponentContainer;

pub type DynService = Arc<dyn Any + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstantiationMode {
    Lazy,
    Eager,
    Explicit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentType {
    Public,
    Private,
    Version,
}

#[derive(Debug, Clone, Default)]
pub struct InstanceFactoryOptions {
    pub instance_identifier: Option<String>,
    pub options: Value,
}

impl InstanceFactoryOptions {
    pub fn new(instance_identifier: Option<String>, options: Value) -> Self {
        Self {
            instance_identifier,
            options,
        }
    }
}

pub type InstanceFactory = Arc<
    dyn Fn(&ComponentContainer, InstanceFactoryOptions) -> Result<DynService, ComponentError>
        + Send
        + Sync,
>;
pub type OnInstanceCreatedCallback =
    Arc<dyn Fn(&ComponentContainer, &str, &DynService) + Send + Sync>;

#[derive(Debug)]
pub enum ComponentError {
    MismatchingComponent { expected: String, found: String },
    ComponentAlreadyProvided { name: String },
    ComponentNotRegistered { name: String },
    InstanceAlreadyInitialized { name: String, identifier: String },
    InitializationFailed { name: String, reason: String },
    InstanceUnavailable { name: String },
}

impl fmt::Display for ComponentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ComponentError::MismatchingComponent { expected, found } => {
                write!(
                    f,
                    "Component {found} cannot satisfy provider for {expected}"
                )
            }
            ComponentError::ComponentAlreadyProvided { name } => {
                write!(f, "Component {name} has already been registered")
            }
            ComponentError::ComponentNotRegistered { name } => {
                write!(f, "Component {name} has not been registered yet")
            }
            ComponentError::InstanceAlreadyInitialized { name, identifier } => {
                write!(f, "{name}({identifier}) has already been initialized")
            }
            ComponentError::InitializationFailed { name, reason } => {
                write!(f, "Component {name} failed to initialize: {reason}")
            }
            ComponentError::InstanceUnavailable { name } => {
                write!(f, "Service {name} is not available")
            }
        }
    }
}

impl std::error::Error for ComponentError {}
