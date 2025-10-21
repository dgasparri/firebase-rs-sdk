use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallationsErrorCode {
    InvalidArgument,
    Internal,
    RequestFailed,
}

impl InstallationsErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            InstallationsErrorCode::InvalidArgument => "installations/invalid-argument",
            InstallationsErrorCode::Internal => "installations/internal",
            InstallationsErrorCode::RequestFailed => "installations/request-failed",
        }
    }
}

#[derive(Clone, Debug)]
pub struct InstallationsError {
    pub code: InstallationsErrorCode,
    message: String,
}

impl InstallationsError {
    pub fn new(code: InstallationsErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code_str(&self) -> &'static str {
        self.code.as_str()
    }
}

impl Display for InstallationsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code_str())
    }
}

impl std::error::Error for InstallationsError {}

pub type InstallationsResult<T> = Result<T, InstallationsError>;

pub fn invalid_argument(message: impl Into<String>) -> InstallationsError {
    InstallationsError::new(InstallationsErrorCode::InvalidArgument, message)
}

pub fn internal_error(message: impl Into<String>) -> InstallationsError {
    InstallationsError::new(InstallationsErrorCode::Internal, message)
}

pub fn request_failed(message: impl Into<String>) -> InstallationsError {
    InstallationsError::new(InstallationsErrorCode::RequestFailed, message)
}
