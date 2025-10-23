use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessagingErrorCode {
    TokenDeletionFailed,
    InvalidArgument,
    Internal,
    PermissionBlocked,
    AvailableInWindow,
    UnsupportedBrowser,
    FailedDefaultRegistration,
    InvalidServiceWorkerRegistration,
    TokenSubscribeFailed,
    TokenUnsubscribeFailed,
    TokenSubscribeNoToken,
    TokenUpdateFailed,
    TokenUpdateNoToken,
}

impl MessagingErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessagingErrorCode::TokenDeletionFailed => "messaging/token-deletion-failed",
            MessagingErrorCode::InvalidArgument => "messaging/invalid-argument",
            MessagingErrorCode::Internal => "messaging/internal",
            MessagingErrorCode::PermissionBlocked => "messaging/permission-blocked",
            MessagingErrorCode::AvailableInWindow => "messaging/available-in-window",
            MessagingErrorCode::UnsupportedBrowser => "messaging/unsupported-browser",
            MessagingErrorCode::FailedDefaultRegistration => {
                "messaging/failed-service-worker-registration"
            }
            MessagingErrorCode::InvalidServiceWorkerRegistration => {
                "messaging/invalid-sw-registration"
            }
            MessagingErrorCode::TokenSubscribeFailed => "messaging/token-subscribe-failed",
            MessagingErrorCode::TokenUnsubscribeFailed => "messaging/token-unsubscribe-failed",
            MessagingErrorCode::TokenSubscribeNoToken => "messaging/token-subscribe-no-token",
            MessagingErrorCode::TokenUpdateFailed => "messaging/token-update-failed",
            MessagingErrorCode::TokenUpdateNoToken => "messaging/token-update-no-token",
        }
    }
}

#[derive(Clone, Debug)]
pub struct MessagingError {
    pub code: MessagingErrorCode,
    message: String,
}

impl MessagingError {
    pub fn new(code: MessagingErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code_str(&self) -> &'static str {
        self.code.as_str()
    }
}

impl Display for MessagingError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code_str())
    }
}

impl std::error::Error for MessagingError {}

pub type MessagingResult<T> = Result<T, MessagingError>;

pub fn invalid_argument(message: impl Into<String>) -> MessagingError {
    MessagingError::new(MessagingErrorCode::InvalidArgument, message)
}

pub fn internal_error(message: impl Into<String>) -> MessagingError {
    MessagingError::new(MessagingErrorCode::Internal, message)
}

pub fn token_deletion_failed(message: impl Into<String>) -> MessagingError {
    MessagingError::new(MessagingErrorCode::TokenDeletionFailed, message)
}

pub fn permission_blocked(message: impl Into<String>) -> MessagingError {
    MessagingError::new(MessagingErrorCode::PermissionBlocked, message)
}

pub fn available_in_window(message: impl Into<String>) -> MessagingError {
    MessagingError::new(MessagingErrorCode::AvailableInWindow, message)
}

pub fn unsupported_browser(message: impl Into<String>) -> MessagingError {
    MessagingError::new(MessagingErrorCode::UnsupportedBrowser, message)
}

pub fn failed_default_registration(message: impl Into<String>) -> MessagingError {
    MessagingError::new(MessagingErrorCode::FailedDefaultRegistration, message)
}

pub fn invalid_service_worker_registration(message: impl Into<String>) -> MessagingError {
    MessagingError::new(
        MessagingErrorCode::InvalidServiceWorkerRegistration,
        message,
    )
}

pub fn token_subscribe_failed(message: impl Into<String>) -> MessagingError {
    MessagingError::new(MessagingErrorCode::TokenSubscribeFailed, message)
}

pub fn token_unsubscribe_failed(message: impl Into<String>) -> MessagingError {
    MessagingError::new(MessagingErrorCode::TokenUnsubscribeFailed, message)
}

pub fn token_subscribe_no_token() -> MessagingError {
    MessagingError::new(
        MessagingErrorCode::TokenSubscribeNoToken,
        "FCM returned no token when subscribing the user to push.",
    )
}

pub fn token_update_failed(message: impl Into<String>) -> MessagingError {
    MessagingError::new(MessagingErrorCode::TokenUpdateFailed, message)
}

pub fn token_update_no_token() -> MessagingError {
    MessagingError::new(
        MessagingErrorCode::TokenUpdateNoToken,
        "FCM returned no token when updating the user to push.",
    )
}
