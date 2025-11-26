use std::fmt;
use std::time::{Duration, SystemTimeError};

use crate::app_check::util::format_duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppCheckError {
    AlreadyInitialized { app_name: String },
    UseBeforeActivation { app_name: String },
    TokenFetchFailed { message: String },
    InvalidConfiguration { message: String },
    ProviderError { message: String },
    FetchNetworkError { message: String },
    FetchParseError { message: String },
    FetchStatusError { http_status: u16 },
    RecaptchaError { message: Option<String> },
    InitialThrottle { http_status: u16, retry_after: Duration },
    Throttled { http_status: u16, retry_after: Duration },
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
            AppCheckError::UseBeforeActivation { app_name } => {
                write!(f, "App Check used before initialize_app_check() for Firebase app '{app_name}'")
            }
            AppCheckError::TokenFetchFailed { message } => {
                write!(f, "Failed to fetch App Check token: {message}")
            }
            AppCheckError::InvalidConfiguration { message } => {
                write!(f, "Invalid App Check configuration: {message}")
            }
            AppCheckError::ProviderError { message } => {
                write!(f, "App Check provider error: {message}")
            }
            AppCheckError::FetchNetworkError { message } => {
                write!(f, "Failed to reach App Check server: {message}")
            }
            AppCheckError::FetchParseError { message } => {
                write!(f, "Failed to parse App Check response: {message}")
            }
            AppCheckError::FetchStatusError { http_status } => {
                write!(f, "App Check server returned HTTP status {http_status}")
            }
            AppCheckError::RecaptchaError { message } => match message {
                Some(message) => write!(f, "reCAPTCHA error: {message}"),
                None => write!(f, "reCAPTCHA error"),
            },
            AppCheckError::InitialThrottle {
                http_status,
                retry_after,
            } => {
                let formatted = format_duration(*retry_after);
                write!(
                    f,
                    "Request temporarily blocked after HTTP {http_status}; retry after {formatted}",
                )
            }
            AppCheckError::Throttled {
                http_status,
                retry_after,
            } => {
                let formatted = format_duration(*retry_after);
                write!(
                    f,
                    "Requests throttled due to previous HTTP {http_status}; retry after {formatted}",
                )
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
