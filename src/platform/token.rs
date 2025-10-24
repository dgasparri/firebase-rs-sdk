use std::error::Error;
use std::fmt;

use async_trait::async_trait;

/// Error type returned by async token providers when token acquisition fails.
#[derive(Debug, Clone)]
pub struct TokenError {
    message: String,
}

impl TokenError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn from_error(err: impl Error) -> Self {
        Self::new(err.to_string())
    }
}

impl fmt::Display for TokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for TokenError {}

/// Shared trait used by modules that need to retrieve auth/app-check tokens asynchronously.
#[async_trait]
pub trait AsyncTokenProvider: Send + Sync {
    async fn get_token(&self, force_refresh: bool) -> Result<Option<String>, TokenError>;
}
