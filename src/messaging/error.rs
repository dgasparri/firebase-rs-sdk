use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessagingErrorCode {
    TokenDeletionFailed,
    InvalidArgument,
    Internal,
}

impl MessagingErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessagingErrorCode::TokenDeletionFailed => "messaging/token-deletion-failed",
            MessagingErrorCode::InvalidArgument => "messaging/invalid-argument",
            MessagingErrorCode::Internal => "messaging/internal",
        }
    }
}

#[derive(Clone, Debug)]
pub struct MessagingError {
    pub code: MessagingErrorCode,
    message: String,
}

impl MessagingError {
    pub fn new(code: MessagingErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code_str(&self) -> &'static str {
        self.code.as_str()
    }
}

impl Display for MessagingError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code_str())
    }
}

impl std::error::Error for MessagingError {}

pub type MessagingResult<T> = Result<T, MessagingError>;

pub fn invalid_argument(message: impl Into<String>) -> MessagingError {
    MessagingError::new(MessagingErrorCode::InvalidArgument, message)
}

pub fn internal_error(message: impl Into<String>) -> MessagingError {
    MessagingError::new(MessagingErrorCode::Internal, message)
}

pub fn token_deletion_failed(message: impl Into<String>) -> MessagingError {
    MessagingError::new(MessagingErrorCode::TokenDeletionFailed, message)
}
