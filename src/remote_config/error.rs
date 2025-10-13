use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteConfigErrorCode {
    InvalidArgument,
    Internal,
}

impl RemoteConfigErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            RemoteConfigErrorCode::InvalidArgument => "remote-config/invalid-argument",
            RemoteConfigErrorCode::Internal => "remote-config/internal",
        }
    }
}

#[derive(Clone, Debug)]
pub struct RemoteConfigError {
    pub code: RemoteConfigErrorCode,
    message: String,
}

impl RemoteConfigError {
    pub fn new(code: RemoteConfigErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code_str(&self) -> &'static str {
        self.code.as_str()
    }
}

impl Display for RemoteConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code_str())
    }
}

impl std::error::Error for RemoteConfigError {}

pub type RemoteConfigResult<T> = Result<T, RemoteConfigError>;

pub fn invalid_argument(message: impl Into<String>) -> RemoteConfigError {
    RemoteConfigError::new(RemoteConfigErrorCode::InvalidArgument, message)
}

pub fn internal_error(message: impl Into<String>) -> RemoteConfigError {
    RemoteConfigError::new(RemoteConfigErrorCode::Internal, message)
}
