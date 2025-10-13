use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DatabaseErrorCode {
    InvalidArgument,
    Internal,
    PermissionDenied,
}

impl DatabaseErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            DatabaseErrorCode::InvalidArgument => "database/invalid-argument",
            DatabaseErrorCode::Internal => "database/internal",
            DatabaseErrorCode::PermissionDenied => "database/permission-denied",
        }
    }
}

#[derive(Clone, Debug)]
pub struct DatabaseError {
    pub code: DatabaseErrorCode,
    message: String,
}

impl DatabaseError {
    pub fn new(code: DatabaseErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code_str(&self) -> &'static str {
        self.code.as_str()
    }
}

impl Display for DatabaseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code_str())
    }
}

impl std::error::Error for DatabaseError {}

pub type DatabaseResult<T> = Result<T, DatabaseError>;

pub fn invalid_argument(message: impl Into<String>) -> DatabaseError {
    DatabaseError::new(DatabaseErrorCode::InvalidArgument, message)
}

pub fn internal_error(message: impl Into<String>) -> DatabaseError {
    DatabaseError::new(DatabaseErrorCode::Internal, message)
}

pub fn permission_denied(message: impl Into<String>) -> DatabaseError {
    DatabaseError::new(DatabaseErrorCode::PermissionDenied, message)
}
