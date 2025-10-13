use std::fmt;
use std::time::SystemTimeError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppCheckError {
    AlreadyInitialized { app_name: String },
    UseBeforeActivation { app_name: String },
    TokenFetchFailed { message: String },
    InvalidConfiguration { message: String },
    ProviderError { message: String },
    TokenExpired,
    Internal(String),
}

pub type AppCheckResult<T> = Result<T, AppCheckError>;

impl fmt::Display for AppCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppCheckError::AlreadyInitialized { app_name } => write!(
                f,
                "App Check already initialized for Firebase app '{app_name}' with different options"
            ),
            AppCheckError::UseBeforeActivation { app_name } => write!(
                f,
                "App Check used before initialize_app_check() for Firebase app '{app_name}'"
            ),
            AppCheckError::TokenFetchFailed { message } => {
                write!(f, "Failed to fetch App Check token: {message}")
            }
            AppCheckError::InvalidConfiguration { message } => {
                write!(f, "Invalid App Check configuration: {message}")
            }
            AppCheckError::ProviderError { message } => {
                write!(f, "App Check provider error: {message}")
            }
            AppCheckError::TokenExpired => {
                write!(f, "App Check token has expired")
            }
            AppCheckError::Internal(message) => {
                write!(f, "Internal App Check error: {message}")
            }
        }
    }
}

impl std::error::Error for AppCheckError {}

impl From<SystemTimeError> for AppCheckError {
    fn from(error: SystemTimeError) -> Self {
        AppCheckError::Internal(error.to_string())
    }
}
