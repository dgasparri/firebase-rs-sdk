use crate::app::AppError;
use crate::auth::types::MultiFactorError;
use crate::util::FirebaseError;
use std::borrow::Cow;
use std::fmt;

pub type AuthResult<T> = Result<T, AuthError>;

#[derive(Debug, Clone)]
pub enum AuthError {
    Firebase(FirebaseError),
    App(AppError),
    Network(String),
    InvalidCredential(String),
    NotImplemented(&'static str),
    MultiFactorRequired(MultiFactorError),
    MultiFactor(MultiFactorAuthError),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::Firebase(err) => write!(f, "{err}"),
            AuthError::App(err) => write!(f, "{err}"),
            AuthError::Network(message) => write!(f, "Network error: {message}"),
            AuthError::InvalidCredential(message) => write!(f, "Invalid credential: {message}"),
            AuthError::NotImplemented(feature) => write!(f, "{feature} is not implemented"),
            AuthError::MultiFactorRequired(err) => write!(f, "{err}"),
            AuthError::MultiFactor(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for AuthError {}

impl From<FirebaseError> for AuthError {
    fn from(error: FirebaseError) -> Self {
        AuthError::Firebase(error)
    }
}

impl From<AppError> for AuthError {
    fn from(error: AppError) -> Self {
        AuthError::App(error)
    }
}

/// Enumerates multi-factor specific error categories surfaced by Firebase Auth.
///
/// Mirrors the JavaScript [`AuthErrorCode`](https://github.com/firebase/firebase-js-sdk/blob/HEAD/packages/auth/src/core/errors.ts)
/// variants related to multi-factor authentication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultiFactorAuthErrorCode {
    /// The provided multi-factor session (pending credential) is missing from the request.
    MissingSession,
    /// The provided multi-factor session (pending credential) is invalid or expired.
    InvalidSession,
    /// Required multi-factor enrollment data (e.g. enrollment ID) is missing from the request.
    MissingInfo,
    /// The requested multi-factor enrollment could not be found for the current user.
    InfoNotFound,
}

impl MultiFactorAuthErrorCode {
    fn default_message(self) -> &'static str {
        match self {
            MultiFactorAuthErrorCode::MissingSession => {
                "Multi-factor session is required to continue the challenge"
            }
            MultiFactorAuthErrorCode::InvalidSession => {
                "The supplied multi-factor session is no longer valid"
            }
            MultiFactorAuthErrorCode::MissingInfo => {
                "Required multi-factor enrollment information is missing"
            }
            MultiFactorAuthErrorCode::InfoNotFound => {
                "The requested multi-factor enrollment could not be found"
            }
        }
    }
}

/// Represents a typed multi-factor error emitted by Firebase Auth REST endpoints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiFactorAuthError {
    code: MultiFactorAuthErrorCode,
    server_message: Option<String>,
}

impl MultiFactorAuthError {
    /// Creates a new error with the provided code and optional server-supplied message detail.
    pub fn new(code: MultiFactorAuthErrorCode, server_message: Option<String>) -> Self {
        Self {
            code,
            server_message,
        }
    }

    /// Returns the structured multi-factor error code.
    pub fn code(&self) -> MultiFactorAuthErrorCode {
        self.code
    }

    /// Returns the raw server message if Firebase sent extra context.
    pub fn server_message(&self) -> Option<&str> {
        self.server_message.as_deref()
    }
}

impl fmt::Display for MultiFactorAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let base = self.code.default_message();
        match self.server_message() {
            Some(detail) if !detail.is_empty() && detail != base => {
                write!(f, "{base} (server message: {detail})")
            }
            _ => write!(f, "{base}"),
        }
    }
}

/// Attempts to convert a REST error message into a typed multi-factor [`AuthError`].
pub(crate) fn map_mfa_error_code(message: &str) -> Option<AuthError> {
    let (raw_code, detail) = split_error_message(message);
    let normalized = normalize_error_code(raw_code);

    let code = match normalized.as_ref() {
        "INVALID_MFA_SESSION"
        | "INVALID_MFA_PENDING_CREDENTIAL"
        | "INVALID_MULTI_FACTOR_SESSION"
        | "INVALID_MULTI_FACTOR_PENDING_CREDENTIAL" => MultiFactorAuthErrorCode::InvalidSession,
        "MISSING_MFA_SESSION"
        | "MISSING_MFA_PENDING_CREDENTIAL"
        | "MISSING_MULTI_FACTOR_SESSION"
        | "MISSING_MULTI_FACTOR_PENDING_CREDENTIAL" => MultiFactorAuthErrorCode::MissingSession,
        "MISSING_MFA_INFO"
        | "MISSING_MFA_ENROLLMENT_ID"
        | "MISSING_MULTI_FACTOR_INFO"
        | "MISSING_MULTI_FACTOR_ENROLLMENT_ID" => MultiFactorAuthErrorCode::MissingInfo,
        "MFA_INFO_NOT_FOUND"
        | "MFA_ENROLLMENT_NOT_FOUND"
        | "MULTI_FACTOR_INFO_NOT_FOUND"
        | "MULTI_FACTOR_ENROLLMENT_NOT_FOUND" => MultiFactorAuthErrorCode::InfoNotFound,
        _ => return None,
    };

    let server_message = detail.map(|value| value.to_string()).or_else(|| {
        if raw_code.is_empty() {
            None
        } else {
            Some(raw_code.to_string())
        }
    });
    Some(AuthError::MultiFactor(MultiFactorAuthError::new(
        code,
        server_message,
    )))
}

fn split_error_message(message: &str) -> (&str, Option<&str>) {
    match message.split_once(':') {
        Some((code, rest)) => (code.trim(), Some(rest.trim())),
        None => (message.trim(), None),
    }
}

fn normalize_error_code(code: &str) -> Cow<'_, str> {
    let stripped = code.trim();
    let without_prefix = stripped.strip_prefix("auth/").unwrap_or(stripped);
    if without_prefix
        .chars()
        .all(|c| c.is_ascii_uppercase() || c == '_')
    {
        Cow::Borrowed(without_prefix)
    } else {
        let mut candidate = without_prefix
            .chars()
            .map(|ch| match ch {
                '-' => '_',
                '/' => '_',
                _ => ch,
            })
            .collect::<String>();
        candidate.make_ascii_uppercase();
        Cow::Owned(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_mfa_error_code_handles_pending_credential() {
        let error = map_mfa_error_code("MISSING_MFA_PENDING_CREDENTIAL");
        match error {
            Some(AuthError::MultiFactor(err)) => {
                assert_eq!(err.code(), MultiFactorAuthErrorCode::MissingSession);
                assert_eq!(err.server_message(), Some("MISSING_MFA_PENDING_CREDENTIAL"));
            }
            other => panic!("unexpected mapping result: {other:?}"),
        }
    }

    #[test]
    fn map_mfa_error_code_accepts_auth_prefixed_values() {
        let error = map_mfa_error_code("auth/multi-factor-info-not-found");
        match error {
            Some(AuthError::MultiFactor(err)) => {
                assert_eq!(err.code(), MultiFactorAuthErrorCode::InfoNotFound);
            }
            other => panic!("unexpected mapping result: {other:?}"),
        }
    }

    #[test]
    fn map_mfa_error_code_returns_none_for_unknown_codes() {
        assert!(map_mfa_error_code("SOME_OTHER_ERROR").is_none());
    }
}
