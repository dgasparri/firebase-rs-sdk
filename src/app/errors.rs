use std::fmt;

use crate::component::types::ComponentError as ProviderComponentError;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    NoApp { app_name: String },
    BadAppName { app_name: String },
    DuplicateApp { app_name: String },
    AppDeleted { app_name: String },
    ServerAppDeleted,
    NoOptions,
    InvalidAppArgument { app_name: String },
    InvalidLogArgument,
    FinalizationRegistryNotSupported,
    InvalidServerAppEnvironment,
    ComponentFailure { component: String, message: String },
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::NoApp { app_name } => {
                write!(f, "No Firebase App '{app_name}' has been created - call initialize_app() first")
            }
            AppError::BadAppName { app_name } => {
                write!(f, "Illegal App name: '{app_name}'")
            }
            AppError::DuplicateApp { app_name } => write!(
                f,
                "Firebase App named '{app_name}' already exists with different options or config"
            ),
            AppError::AppDeleted { app_name } => {
                write!(f, "Firebase App named '{app_name}' already deleted")
            }
            AppError::ServerAppDeleted => {
                write!(f, "Firebase Server App has been deleted")
            }
            AppError::NoOptions => write!(f, "Need to provide options when not being deployed to hosting via source."),
            AppError::InvalidAppArgument { app_name } => {
                write!(f, "firebase.{app_name}() takes either no argument or a Firebase App instance.")
            }
            AppError::InvalidLogArgument => {
                write!(f, "First argument to on_log must be None or a function.")
            }
            AppError::FinalizationRegistryNotSupported => write!(
                f,
                "FirebaseServerApp release_on_deref defined but runtime lacks FinalizationRegistry support."
            ),
            AppError::InvalidServerAppEnvironment => {
                write!(f, "FirebaseServerApp is not for use in browser environments.")
            }
            AppError::ComponentFailure { component, message } => {
                write!(f, "Component {component} error: {message}")
            }
        }
    }
}

impl std::error::Error for AppError {}

impl From<ProviderComponentError> for AppError {
    fn from(err: ProviderComponentError) -> Self {
        match err {
            ProviderComponentError::MismatchingComponent { expected, found } => AppError::ComponentFailure {
                component: found,
                message: format!("does not satisfy provider for {expected}"),
            },
            ProviderComponentError::ComponentAlreadyProvided { name } => AppError::ComponentFailure {
                component: name,
                message: "component already provided".to_string(),
            },
            ProviderComponentError::ComponentNotRegistered { name } => AppError::ComponentFailure {
                component: name,
                message: "component not registered".to_string(),
            },
            ProviderComponentError::InstanceAlreadyInitialized { name, identifier } => AppError::ComponentFailure {
                component: name,
                message: format!("instance {identifier} already initialized"),
            },
            ProviderComponentError::InitializationFailed { name, reason } => AppError::ComponentFailure {
                component: name,
                message: reason,
            },
            ProviderComponentError::InstanceUnavailable { name } => AppError::ComponentFailure {
                component: name,
                message: "instance unavailable".to_string(),
            },
        }
    }
}
