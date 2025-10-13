use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataConnectErrorCode {
    InvalidArgument,
    Internal,
}

impl DataConnectErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            DataConnectErrorCode::InvalidArgument => "data-connect/invalid-argument",
            DataConnectErrorCode::Internal => "data-connect/internal",
        }
    }
}

#[derive(Clone, Debug)]
pub struct DataConnectError {
    pub code: DataConnectErrorCode,
    message: String,
}

impl DataConnectError {
    pub fn new(code: DataConnectErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code_str(&self) -> &'static str {
        self.code.as_str()
    }
}

impl Display for DataConnectError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code_str())
    }
}

impl std::error::Error for DataConnectError {}

pub type DataConnectResult<T> = Result<T, DataConnectError>;

pub fn invalid_argument(message: impl Into<String>) -> DataConnectError {
    DataConnectError::new(DataConnectErrorCode::InvalidArgument, message)
}

pub fn internal_error(message: impl Into<String>) -> DataConnectError {
    DataConnectError::new(DataConnectErrorCode::Internal, message)
}
