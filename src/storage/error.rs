use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageErrorCode {
    Unknown,
    InvalidUrl,
    InvalidDefaultBucket,
    NoDefaultBucket,
    InvalidArgument,
    AppDeleted,
    InvalidRootOperation,
    InternalError,
    UnsupportedEnvironment,
    NoDownloadUrl,
}

impl StorageErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            StorageErrorCode::Unknown => "storage/unknown",
            StorageErrorCode::InvalidUrl => "storage/invalid-url",
            StorageErrorCode::InvalidDefaultBucket => "storage/invalid-default-bucket",
            StorageErrorCode::NoDefaultBucket => "storage/no-default-bucket",
            StorageErrorCode::InvalidArgument => "storage/invalid-argument",
            StorageErrorCode::AppDeleted => "storage/app-deleted",
            StorageErrorCode::InvalidRootOperation => "storage/invalid-root-operation",
            StorageErrorCode::InternalError => "storage/internal-error",
            StorageErrorCode::UnsupportedEnvironment => "storage/unsupported-environment",
            StorageErrorCode::NoDownloadUrl => "storage/no-download-url",
        }
    }
}

#[derive(Debug, Clone)]
pub struct StorageError {
    pub code: StorageErrorCode,
    message: String,
    pub status: Option<u16>,
    pub server_response: Option<String>,
}

impl StorageError {
    pub fn new(code: StorageErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            status: None,
            server_response: None,
        }
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.status = Some(status);
        self
    }

    pub fn with_server_response(mut self, response: impl Into<String>) -> Self {
        self.server_response = Some(response.into());
        self
    }

    pub fn code_str(&self) -> &'static str {
        self.code.as_str()
    }
}

impl Display for StorageError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(server) = &self.server_response {
            write!(f, "{} ({}): {}", self.message, self.code_str(), server)
        } else {
            write!(f, "{} ({})", self.message, self.code_str())
        }
    }
}

impl Error for StorageError {}

pub type StorageResult<T> = Result<T, StorageError>;

pub fn unknown_error() -> StorageError {
    StorageError::new(
        StorageErrorCode::Unknown,
        "An unknown error occurred; check the error payload for details.",
    )
}

pub fn invalid_url(url: &str) -> StorageError {
    StorageError::new(
        StorageErrorCode::InvalidUrl,
        format!("Invalid storage URL: {url}"),
    )
}

pub fn invalid_default_bucket(bucket: &str) -> StorageError {
    StorageError::new(
        StorageErrorCode::InvalidDefaultBucket,
        format!("Invalid default bucket: {bucket}"),
    )
}

pub fn no_default_bucket() -> StorageError {
    StorageError::new(
        StorageErrorCode::NoDefaultBucket,
        "No default storage bucket configured on this Firebase app.",
    )
}

pub fn invalid_argument(message: impl Into<String>) -> StorageError {
    StorageError::new(StorageErrorCode::InvalidArgument, message)
}

pub fn app_deleted() -> StorageError {
    StorageError::new(
        StorageErrorCode::AppDeleted,
        "The Firebase app associated with this Storage instance was deleted.",
    )
}

pub fn invalid_root_operation(operation: &str) -> StorageError {
    StorageError::new(
        StorageErrorCode::InvalidRootOperation,
        format!("'{operation}' cannot be performed on the storage root reference."),
    )
}

pub fn unsupported_environment(message: impl Into<String>) -> StorageError {
    StorageError::new(StorageErrorCode::UnsupportedEnvironment, message)
}

pub fn internal_error(message: impl Into<String>) -> StorageError {
    StorageError::new(StorageErrorCode::InternalError, message)
}

pub fn no_download_url() -> StorageError {
    StorageError::new(
        StorageErrorCode::NoDownloadUrl,
        "The requested object does not expose a download URL.",
    )
}
