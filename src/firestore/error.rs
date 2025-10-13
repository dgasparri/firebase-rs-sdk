use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FirestoreErrorCode {
    InvalidArgument,
    MissingProjectId,
    Internal,
    NotFound,
    PermissionDenied,
    Unauthenticated,
    Unavailable,
    DeadlineExceeded,
    ResourceExhausted,
}

impl FirestoreErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            FirestoreErrorCode::InvalidArgument => "firestore/invalid-argument",
            FirestoreErrorCode::MissingProjectId => "firestore/missing-project-id",
            FirestoreErrorCode::Internal => "firestore/internal",
            FirestoreErrorCode::NotFound => "firestore/not-found",
            FirestoreErrorCode::PermissionDenied => "firestore/permission-denied",
            FirestoreErrorCode::Unauthenticated => "firestore/unauthenticated",
            FirestoreErrorCode::Unavailable => "firestore/unavailable",
            FirestoreErrorCode::DeadlineExceeded => "firestore/deadline-exceeded",
            FirestoreErrorCode::ResourceExhausted => "firestore/resource-exhausted",
        }
    }
}

#[derive(Clone, Debug)]
pub struct FirestoreError {
    pub code: FirestoreErrorCode,
    message: String,
}

impl FirestoreError {
    pub fn new(code: FirestoreErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code_str(&self) -> &'static str {
        self.code.as_str()
    }
}

impl Display for FirestoreError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code_str())
    }
}

impl Error for FirestoreError {}

pub type FirestoreResult<T> = Result<T, FirestoreError>;

pub fn invalid_argument(message: impl Into<String>) -> FirestoreError {
    FirestoreError::new(FirestoreErrorCode::InvalidArgument, message)
}

pub fn missing_project_id() -> FirestoreError {
    FirestoreError::new(
        FirestoreErrorCode::MissingProjectId,
        "Firebase options must include a project_id to use Firestore",
    )
}

pub fn internal_error(message: impl Into<String>) -> FirestoreError {
    FirestoreError::new(FirestoreErrorCode::Internal, message)
}

pub fn not_found(message: impl Into<String>) -> FirestoreError {
    FirestoreError::new(FirestoreErrorCode::NotFound, message)
}

pub fn permission_denied(message: impl Into<String>) -> FirestoreError {
    FirestoreError::new(FirestoreErrorCode::PermissionDenied, message)
}

pub fn unauthenticated(message: impl Into<String>) -> FirestoreError {
    FirestoreError::new(FirestoreErrorCode::Unauthenticated, message)
}

pub fn unavailable(message: impl Into<String>) -> FirestoreError {
    FirestoreError::new(FirestoreErrorCode::Unavailable, message)
}

pub fn deadline_exceeded(message: impl Into<String>) -> FirestoreError {
    FirestoreError::new(FirestoreErrorCode::DeadlineExceeded, message)
}

pub fn resource_exhausted(message: impl Into<String>) -> FirestoreError {
    FirestoreError::new(FirestoreErrorCode::ResourceExhausted, message)
}
