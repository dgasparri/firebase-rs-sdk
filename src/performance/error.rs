use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PerformanceErrorCode {
    InvalidArgument,
    Internal,
}

impl PerformanceErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            PerformanceErrorCode::InvalidArgument => "performance/invalid-argument",
            PerformanceErrorCode::Internal => "performance/internal",
        }
    }
}

#[derive(Clone, Debug)]
pub struct PerformanceError {
    pub code: PerformanceErrorCode,
    message: String,
}

impl PerformanceError {
    pub fn new(code: PerformanceErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code_str(&self) -> &'static str {
        self.code.as_str()
    }
}

impl Display for PerformanceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code_str())
    }
}

impl std::error::Error for PerformanceError {}

pub type PerformanceResult<T> = Result<T, PerformanceError>;

pub fn invalid_argument(message: impl Into<String>) -> PerformanceError {
    PerformanceError::new(PerformanceErrorCode::InvalidArgument, message)
}

pub fn internal_error(message: impl Into<String>) -> PerformanceError {
    PerformanceError::new(PerformanceErrorCode::Internal, message)
}
