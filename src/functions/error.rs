use std::fmt::{Display, Formatter};

use serde_json::Value as JsonValue;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionsErrorCode {
    Ok,
    Cancelled,
    Unknown,
    Internal,
    InvalidArgument,
    DeadlineExceeded,
    NotFound,
    AlreadyExists,
    PermissionDenied,
    Unauthenticated,
    ResourceExhausted,
    FailedPrecondition,
    Aborted,
    OutOfRange,
    Unimplemented,
    Unavailable,
    DataLoss,
}

impl FunctionsErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            FunctionsErrorCode::Ok => "functions/ok",
            FunctionsErrorCode::Cancelled => "functions/cancelled",
            FunctionsErrorCode::Unknown => "functions/unknown",
            FunctionsErrorCode::Internal => "functions/internal",
            FunctionsErrorCode::InvalidArgument => "functions/invalid-argument",
            FunctionsErrorCode::DeadlineExceeded => "functions/deadline-exceeded",
            FunctionsErrorCode::NotFound => "functions/not-found",
            FunctionsErrorCode::AlreadyExists => "functions/already-exists",
            FunctionsErrorCode::PermissionDenied => "functions/permission-denied",
            FunctionsErrorCode::Unauthenticated => "functions/unauthenticated",
            FunctionsErrorCode::ResourceExhausted => "functions/resource-exhausted",
            FunctionsErrorCode::FailedPrecondition => "functions/failed-precondition",
            FunctionsErrorCode::Aborted => "functions/aborted",
            FunctionsErrorCode::OutOfRange => "functions/out-of-range",
            FunctionsErrorCode::Unimplemented => "functions/unimplemented",
            FunctionsErrorCode::Unavailable => "functions/unavailable",
            FunctionsErrorCode::DataLoss => "functions/data-loss",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            FunctionsErrorCode::Ok => "ok",
            FunctionsErrorCode::Cancelled => "cancelled",
            FunctionsErrorCode::Unknown => "unknown",
            FunctionsErrorCode::Internal => "internal",
            FunctionsErrorCode::InvalidArgument => "invalid-argument",
            FunctionsErrorCode::DeadlineExceeded => "deadline-exceeded",
            FunctionsErrorCode::NotFound => "not-found",
            FunctionsErrorCode::AlreadyExists => "already-exists",
            FunctionsErrorCode::PermissionDenied => "permission-denied",
            FunctionsErrorCode::Unauthenticated => "unauthenticated",
            FunctionsErrorCode::ResourceExhausted => "resource-exhausted",
            FunctionsErrorCode::FailedPrecondition => "failed-precondition",
            FunctionsErrorCode::Aborted => "aborted",
            FunctionsErrorCode::OutOfRange => "out-of-range",
            FunctionsErrorCode::Unimplemented => "unimplemented",
            FunctionsErrorCode::Unavailable => "unavailable",
            FunctionsErrorCode::DataLoss => "data-loss",
        }
    }
}

#[derive(Clone, Debug)]
pub struct FunctionsError {
    pub code: FunctionsErrorCode,
    message: String,
    details: Option<JsonValue>,
}

impl FunctionsError {
    pub fn new(code: FunctionsErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn code_str(&self) -> &'static str {
        self.code.as_str()
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn details(&self) -> Option<&JsonValue> {
        self.details.as_ref()
    }

    pub fn with_details(
        code: FunctionsErrorCode,
        message: impl Into<String>,
        details: Option<JsonValue>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            details,
        }
    }
}

impl Display for FunctionsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code_str())
    }
}

impl std::error::Error for FunctionsError {}

pub type FunctionsResult<T> = Result<T, FunctionsError>;

pub fn invalid_argument(message: impl Into<String>) -> FunctionsError {
    FunctionsError::new(FunctionsErrorCode::InvalidArgument, message)
}

pub fn internal_error(message: impl Into<String>) -> FunctionsError {
    FunctionsError::new(FunctionsErrorCode::Internal, message)
}

pub(crate) fn error_for_http_response(
    status: u16,
    body: Option<&JsonValue>,
) -> Option<FunctionsError> {
    use FunctionsErrorCode as Code;

    let mut code = code_for_http_status(status);
    let mut message: Option<String> = code_message_default(code);
    let mut details: Option<JsonValue> = None;

    if let Some(JsonValue::Object(map)) = body {
        if let Some(error_value) = map.get("error") {
            if let JsonValue::Object(error_obj) = error_value {
                if let Some(JsonValue::String(status_label)) = error_obj.get("status") {
                    match code_for_backend_status(status_label) {
                        Some(mapped) => {
                            code = mapped;
                            message = Some(status_label.clone());
                        }
                        None => {
                            return Some(FunctionsError::new(
                                Code::Internal,
                                "Received unknown error status from Functions backend",
                            ));
                        }
                    }
                }

                if let Some(JsonValue::String(msg)) = error_obj.get("message") {
                    message = Some(msg.clone());
                }

                if let Some(value) = error_obj.get("details") {
                    details = Some(value.clone());
                }
            }
        }
    }

    if matches!(code, Code::Ok) {
        return None;
    }

    Some(FunctionsError::with_details(
        code,
        message.unwrap_or_else(|| code.label().to_string()),
        details,
    ))
}

fn code_message_default(code: FunctionsErrorCode) -> Option<String> {
    match code {
        FunctionsErrorCode::Ok => Some("ok".to_string()),
        FunctionsErrorCode::Internal => Some("internal".to_string()),
        FunctionsErrorCode::InvalidArgument => Some("invalid-argument".to_string()),
        FunctionsErrorCode::DeadlineExceeded => Some("deadline-exceeded".to_string()),
        FunctionsErrorCode::NotFound => Some("not-found".to_string()),
        FunctionsErrorCode::AlreadyExists => Some("already-exists".to_string()),
        FunctionsErrorCode::PermissionDenied => Some("permission-denied".to_string()),
        FunctionsErrorCode::Unauthenticated => Some("unauthenticated".to_string()),
        FunctionsErrorCode::ResourceExhausted => Some("resource-exhausted".to_string()),
        FunctionsErrorCode::FailedPrecondition => Some("failed-precondition".to_string()),
        FunctionsErrorCode::Aborted => Some("aborted".to_string()),
        FunctionsErrorCode::OutOfRange => Some("out-of-range".to_string()),
        FunctionsErrorCode::Unimplemented => Some("unimplemented".to_string()),
        FunctionsErrorCode::Unavailable => Some("unavailable".to_string()),
        FunctionsErrorCode::DataLoss => Some("data-loss".to_string()),
        FunctionsErrorCode::Cancelled => Some("cancelled".to_string()),
        FunctionsErrorCode::Unknown => Some("unknown".to_string()),
    }
}

fn code_for_http_status(status: u16) -> FunctionsErrorCode {
    use FunctionsErrorCode as Code;

    if (200..300).contains(&status) {
        return Code::Ok;
    }

    match status {
        0 => Code::Internal,
        400 => Code::InvalidArgument,
        401 => Code::Unauthenticated,
        403 => Code::PermissionDenied,
        404 => Code::NotFound,
        409 => Code::Aborted,
        429 => Code::ResourceExhausted,
        499 => Code::Cancelled,
        500 => Code::Internal,
        501 => Code::Unimplemented,
        503 => Code::Unavailable,
        504 => Code::DeadlineExceeded,
        _ => Code::Unknown,
    }
}

fn code_for_backend_status(status: &str) -> Option<FunctionsErrorCode> {
    use FunctionsErrorCode as Code;

    match status {
        "OK" => Some(Code::Ok),
        "CANCELLED" => Some(Code::Cancelled),
        "UNKNOWN" => Some(Code::Unknown),
        "INVALID_ARGUMENT" => Some(Code::InvalidArgument),
        "DEADLINE_EXCEEDED" => Some(Code::DeadlineExceeded),
        "NOT_FOUND" => Some(Code::NotFound),
        "ALREADY_EXISTS" => Some(Code::AlreadyExists),
        "PERMISSION_DENIED" => Some(Code::PermissionDenied),
        "UNAUTHENTICATED" => Some(Code::Unauthenticated),
        "RESOURCE_EXHAUSTED" => Some(Code::ResourceExhausted),
        "FAILED_PRECONDITION" => Some(Code::FailedPrecondition),
        "ABORTED" => Some(Code::Aborted),
        "OUT_OF_RANGE" => Some(Code::OutOfRange),
        "UNIMPLEMENTED" => Some(Code::Unimplemented),
        "INTERNAL" => Some(Code::Internal),
        "UNAVAILABLE" => Some(Code::Unavailable),
        "DATA_LOSS" => Some(Code::DataLoss),
        _ => None,
    }
}
