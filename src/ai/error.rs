use std::fmt::{Display, Formatter};

use serde_json::{Map, Value};

use crate::ai::constants::AI_TYPE;

/// Error codes exposed by the Firebase AI SDK.
///
/// Ported from `packages/ai/src/types/error.ts` (`AIErrorCode`) with the addition of the
/// legacy `InvalidArgument` and `Internal` variants that already existed in the Rust stub.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AiErrorCode {
    Error,
    RequestError,
    ResponseError,
    FetchError,
    SessionClosed,
    InvalidContent,
    ApiNotEnabled,
    InvalidSchema,
    NoApiKey,
    NoAppId,
    NoModel,
    NoProjectId,
    ParseFailed,
    Unsupported,
    InvalidArgument,
    Internal,
}

impl AiErrorCode {
    /// Returns the canonical string representation (e.g. `"no-model"`).
    pub fn as_code(&self) -> &'static str {
        match self {
            AiErrorCode::Error => "error",
            AiErrorCode::RequestError => "request-error",
            AiErrorCode::ResponseError => "response-error",
            AiErrorCode::FetchError => "fetch-error",
            AiErrorCode::SessionClosed => "session-closed",
            AiErrorCode::InvalidContent => "invalid-content",
            AiErrorCode::ApiNotEnabled => "api-not-enabled",
            AiErrorCode::InvalidSchema => "invalid-schema",
            AiErrorCode::NoApiKey => "no-api-key",
            AiErrorCode::NoAppId => "no-app-id",
            AiErrorCode::NoModel => "no-model",
            AiErrorCode::NoProjectId => "no-project-id",
            AiErrorCode::ParseFailed => "parse-failed",
            AiErrorCode::Unsupported => "unsupported",
            AiErrorCode::InvalidArgument => "invalid-argument",
            AiErrorCode::Internal => "internal",
        }
    }
}

/// Structured information returned alongside certain errors.
///
/// Ported from `packages/ai/src/types/error.ts` (`ErrorDetails`).
#[derive(Clone, Debug, PartialEq)]
pub struct ErrorDetails {
    pub type_url: Option<String>,
    pub reason: Option<String>,
    pub domain: Option<String>,
    pub metadata: Option<Map<String, Value>>,
}

impl Default for ErrorDetails {
    fn default() -> Self {
        Self {
            type_url: None,
            reason: None,
            domain: None,
            metadata: None,
        }
    }
}

/// Additional error data captured from HTTP responses or provider payloads.
///
/// Ported from `packages/ai/src/types/error.ts` (`CustomErrorData`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CustomErrorData {
    pub status: Option<u16>,
    pub status_text: Option<String>,
    pub response: Option<Value>,
    pub error_details: Vec<ErrorDetails>,
}

impl CustomErrorData {
    pub fn with_status(mut self, status: u16) -> Self {
        self.status = Some(status);
        self
    }

    pub fn with_status_text<S: Into<String>>(mut self, status_text: S) -> Self {
        self.status_text = Some(status_text.into());
        self
    }

    pub fn with_response(mut self, response: Value) -> Self {
        self.response = Some(response);
        self
    }

    pub fn with_error_details(mut self, details: Vec<ErrorDetails>) -> Self {
        self.error_details = details;
        self
    }
}

#[derive(Clone, Debug)]
pub struct AiError {
    pub code: AiErrorCode,
    message: String,
    pub custom_error_data: Option<CustomErrorData>,
}

impl AiError {
    /// Creates a new AI error with the provided code, message, and optional custom data.
    pub fn new(
        code: AiErrorCode,
        message: impl Into<String>,
        custom_error_data: Option<CustomErrorData>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            custom_error_data,
        }
    }

    /// Returns the logical error code (without the `AI/` prefix).
    pub fn code(&self) -> AiErrorCode {
        self.code
    }

    /// Returns the fully qualified error code string (e.g. `AI/no-model`).
    pub fn code_str(&self) -> String {
        format!("{}/{}", AI_TYPE, self.code.as_code())
    }

    /// Returns the human readable error message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Display for AiError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {} ({})", AI_TYPE, self.message, self.code_str())
    }
}

impl std::error::Error for AiError {}

pub type AiResult<T> = Result<T, AiError>;

pub fn invalid_argument(message: impl Into<String>) -> AiError {
    AiError::new(AiErrorCode::InvalidArgument, message, None)
}

pub fn internal_error(message: impl Into<String>) -> AiError {
    AiError::new(AiErrorCode::Internal, message, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_full_code() {
        let err = invalid_argument("Bad input");
        assert_eq!(err.code(), AiErrorCode::InvalidArgument);
        assert_eq!(err.code_str(), "AI/invalid-argument");
        assert_eq!(err.message(), "Bad input");
        assert_eq!(format!("{}", err), "AI: Bad input (AI/invalid-argument)");
    }

    #[test]
    fn supports_custom_data_builders() {
        let details = ErrorDetails {
            type_url: Some("type.googleapis.com/google.rpc.ErrorInfo".into()),
            reason: Some("RATE_LIMIT".into()),
            domain: None,
            metadata: Some(Map::new()),
        };
        let data = CustomErrorData::default()
            .with_status(429)
            .with_status_text("Too Many Requests")
            .with_error_details(vec![details.clone()]);
        let err = AiError::new(
            AiErrorCode::FetchError,
            "quota exceeded",
            Some(data.clone()),
        );
        assert_eq!(err.custom_error_data, Some(data));
        assert!(err.code_str().contains("fetch-error"));
    }
}
