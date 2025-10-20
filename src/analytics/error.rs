use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnalyticsErrorCode {
    InvalidArgument,
    Internal,
    Network,
}

impl AnalyticsErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            AnalyticsErrorCode::InvalidArgument => "analytics/invalid-argument",
            AnalyticsErrorCode::Internal => "analytics/internal",
            AnalyticsErrorCode::Network => "analytics/network",
        }
    }
}

#[derive(Clone, Debug)]
pub struct AnalyticsError {
    pub code: AnalyticsErrorCode,
    message: String,
}

impl AnalyticsError {
    pub fn new(code: AnalyticsErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code_str(&self) -> &'static str {
        self.code.as_str()
    }
}

impl Display for AnalyticsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code_str())
    }
}

impl std::error::Error for AnalyticsError {}

pub type AnalyticsResult<T> = Result<T, AnalyticsError>;

pub fn invalid_argument(message: impl Into<String>) -> AnalyticsError {
    AnalyticsError::new(AnalyticsErrorCode::InvalidArgument, message)
}

pub fn internal_error(message: impl Into<String>) -> AnalyticsError {
    AnalyticsError::new(AnalyticsErrorCode::Internal, message)
}

pub fn network_error(message: impl Into<String>) -> AnalyticsError {
    AnalyticsError::new(AnalyticsErrorCode::Network, message)
}
