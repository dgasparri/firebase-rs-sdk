use crate::app::AppError;
use crate::util::FirebaseError;
use std::fmt;

pub type AuthResult<T> = Result<T, AuthError>;

#[derive(Debug, Clone)]
pub enum AuthError {
    Firebase(FirebaseError),
    App(AppError),
    Network(String),
    InvalidCredential(String),
    NotImplemented(&'static str),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::Firebase(err) => write!(f, "{}", err),
            AuthError::App(err) => write!(f, "{}", err),
            AuthError::Network(message) => write!(f, "Network error: {message}"),
            AuthError::InvalidCredential(message) => write!(f, "Invalid credential: {message}"),
            AuthError::NotImplemented(feature) => write!(f, "{feature} is not implemented"),
        }
    }
}

impl std::error::Error for AuthError {}

impl From<FirebaseError> for AuthError {
    fn from(error: FirebaseError) -> Self {
        AuthError::Firebase(error)
    }
}

impl From<AppError> for AuthError {
    fn from(error: AppError) -> Self {
        AuthError::App(error)
    }
}
