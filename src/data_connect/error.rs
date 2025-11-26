use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::{self, Display, Formatter};

/// Result alias returned by Data Connect APIs.
pub type DataConnectResult<T> = Result<T, DataConnectError>;

/// Enumerates the canonical error codes surfaced by the Data Connect module.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataConnectErrorCode {
    /// The client supplied an invalid argument (missing connector config, etc.).
    InvalidArgument,
    /// The client attempted to reuse an instance in an incompatible configuration.
    AlreadyInitialized,
    /// The service has not been initialized yet.
    NotInitialized,
    /// The current platform does not support the requested feature.
    NotSupported,
    /// The backend rejected the request because authentication was missing/invalid.
    Unauthorized,
    /// The backend returned a GraphQL `errors` payload.
    PartialError,
    /// Any other internal client failure.
    Internal,
    /// Unknown errors reported by the backend or transport layer.
    Other,
}

impl DataConnectErrorCode {
    /// Returns the string form exposed to callers.
    pub fn as_str(&self) -> &'static str {
        match self {
            DataConnectErrorCode::InvalidArgument => "data-connect/invalid-argument",
            DataConnectErrorCode::AlreadyInitialized => "data-connect/already-initialized",
            DataConnectErrorCode::NotInitialized => "data-connect/not-initialized",
            DataConnectErrorCode::NotSupported => "data-connect/not-supported",
            DataConnectErrorCode::Unauthorized => "data-connect/unauthorized",
            DataConnectErrorCode::PartialError => "data-connect/partial-error",
            DataConnectErrorCode::Internal => "data-connect/internal",
            DataConnectErrorCode::Other => "data-connect/other",
        }
    }
}

/// Rich failure type returned by Data Connect operations.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DataConnectError {
    code: DataConnectErrorCode,
    message: String,
    operation_failure: Option<DataConnectOperationFailureResponse>,
}

impl DataConnectError {
    /// Creates a new error with the specified code and message.
    pub fn new(code: DataConnectErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            operation_failure: None,
        }
    }

    /// Creates an error that captures the backend-provided GraphQL payload.
    pub fn with_operation_failure(message: impl Into<String>, response: DataConnectOperationFailureResponse) -> Self {
        Self {
            code: DataConnectErrorCode::PartialError,
            message: message.into(),
            operation_failure: Some(response),
        }
    }

    /// Returns the canonical error code string.
    pub fn code_str(&self) -> &'static str {
        self.code.as_str()
    }

    /// Returns the structured error code.
    pub fn code(&self) -> DataConnectErrorCode {
        self.code
    }

    /// Returns the human readable description.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the backend failure payload when the code is `PartialError`.
    pub fn operation_failure(&self) -> Option<&DataConnectOperationFailureResponse> {
        self.operation_failure.as_ref()
    }
}

impl Display for DataConnectError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.message, self.code.as_str())
    }
}

impl std::error::Error for DataConnectError {}

/// GraphQL error payload mirrored from the JS SDK.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DataConnectOperationFailureResponse {
    /// Partial data returned by the backend, if any.
    pub data: Option<Value>,
    /// The list of GraphQL errors reported for the operation.
    pub errors: Vec<DataConnectOperationFailureResponseErrorInfo>,
}

/// Individual error entry returned by the GraphQL endpoint.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DataConnectOperationFailureResponseErrorInfo {
    /// Error summary message.
    pub message: String,
    /// Path into the GraphQL response that triggered the failure.
    pub path: Vec<DataConnectErrorPathSegment>,
}

/// A single entry in the `path` array from a GraphQL error response.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum DataConnectErrorPathSegment {
    Field(String),
    Index(i64),
}

impl Display for DataConnectErrorPathSegment {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            DataConnectErrorPathSegment::Field(field) => write!(f, "{}", field),
            DataConnectErrorPathSegment::Index(idx) => write!(f, "{}", idx),
        }
    }
}

/// Helper for constructing an invalid argument error.
pub fn invalid_argument(message: impl Into<String>) -> DataConnectError {
    DataConnectError::new(DataConnectErrorCode::InvalidArgument, message)
}

/// Helper for constructing an unauthorized error.
pub fn unauthorized(message: impl Into<String>) -> DataConnectError {
    DataConnectError::new(DataConnectErrorCode::Unauthorized, message)
}

/// Helper for constructing an internal error.
pub fn internal_error(message: impl Into<String>) -> DataConnectError {
    DataConnectError::new(DataConnectErrorCode::Internal, message)
}

/// Helper for surfacing partial/GraphQL errors from the backend.
pub fn operation_error(message: impl Into<String>, response: DataConnectOperationFailureResponse) -> DataConnectError {
    DataConnectError::with_operation_failure(message, response)
}
