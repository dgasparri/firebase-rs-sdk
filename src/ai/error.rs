use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AiErrorCode {
    InvalidArgument,
    Internal,
}

impl AiErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            AiErrorCode::InvalidArgument => "ai/invalid-argument",
            AiErrorCode::Internal => "ai/internal",
        }
    }
}

#[derive(Clone, Debug)]
pub struct AiError {
    pub code: AiErrorCode,
    message: String,
}

impl AiError {
    pub fn new(code: AiErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code_str(&self) -> &'static str {
        self.code.as_str()
    }
}

impl Display for AiError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code_str())
    }
}

impl std::error::Error for AiError {}

pub type AiResult<T> = Result<T, AiError>;

pub fn invalid_argument(message: impl Into<String>) -> AiError {
    AiError::new(AiErrorCode::InvalidArgument, message)
}

pub fn internal_error(message: impl Into<String>) -> AiError {
    AiError::new(AiErrorCode::Internal, message)
}
