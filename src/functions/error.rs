use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FunctionsErrorCode {
    Internal,
    InvalidArgument,
}

impl FunctionsErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            FunctionsErrorCode::Internal => "functions/internal",
            FunctionsErrorCode::InvalidArgument => "functions/invalid-argument",
        }
    }
}

#[derive(Clone, Debug)]
pub struct FunctionsError {
    pub code: FunctionsErrorCode,
    message: String,
}

impl FunctionsError {
    pub fn new(code: FunctionsErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code_str(&self) -> &'static str {
        self.code.as_str()
    }
}

impl Display for FunctionsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code_str())
    }
}

impl std::error::Error for FunctionsError {}

pub type FunctionsResult<T> = Result<T, FunctionsError>;

pub fn invalid_argument(message: impl Into<String>) -> FunctionsError {
    FunctionsError::new(FunctionsErrorCode::InvalidArgument, message)
}

pub fn internal_error(message: impl Into<String>) -> FunctionsError {
    FunctionsError::new(FunctionsErrorCode::Internal, message)
}
