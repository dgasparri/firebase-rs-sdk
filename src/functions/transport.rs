use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

use serde_json::Value as JsonValue;

use crate::functions::error::FunctionsResult;

#[derive(Clone, Debug)]
pub struct CallableRequest {
    pub url: String,
    pub payload: JsonValue,
    pub timeout: Duration,
    pub headers: HashMap<String, String>,
}

impl CallableRequest {
    pub fn new(url: impl Into<String>, payload: JsonValue, timeout: Duration) -> Self {
        Self {
            url: url.into(),
            payload,
            timeout,
            headers: HashMap::new(),
        }
    }
}

pub trait CallableTransport: Send + Sync {
    fn invoke(&self, request: CallableRequest) -> FunctionsResult<JsonValue>;
}

pub fn invoke_callable(request: CallableRequest) -> FunctionsResult<JsonValue> {
    callable_transport().invoke(request)
}

pub fn callable_transport() -> &'static dyn CallableTransport {
    &*TRANSPORT
}

#[cfg(not(target_arch = "wasm32"))]
static TRANSPORT: LazyLock<NativeCallableTransport> = LazyLock::new(NativeCallableTransport::new);

#[cfg(target_arch = "wasm32")]
static TRANSPORT: LazyLock<WasmCallableTransport> = LazyLock::new(WasmCallableTransport::new);

#[cfg(not(target_arch = "wasm32"))]
struct NativeCallableTransport;

#[cfg(not(target_arch = "wasm32"))]
impl NativeCallableTransport {
    fn new() -> Self {
        Self
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl CallableTransport for NativeCallableTransport {
    fn invoke(&self, request: CallableRequest) -> FunctionsResult<JsonValue> {
        native::invoke(request)
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{CallableRequest, JsonValue};
    use crate::functions::error::{
        error_for_http_response, internal_error, invalid_argument, FunctionsError,
        FunctionsErrorCode, FunctionsResult,
    };
    use reqwest::blocking::{Client, Response};
    use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
    use reqwest::StatusCode;
    use std::collections::HashMap;
    use std::sync::LazyLock;

    fn client() -> &'static Client {
        static CLIENT: LazyLock<Client> = LazyLock::new(|| {
            Client::builder()
                .build()
                .expect("Failed to construct reqwest client")
        });
        &CLIENT
    }

    fn build_headers(headers: &HashMap<String, String>) -> FunctionsResult<HeaderMap> {
        let mut map = HeaderMap::new();
        for (key, value) in headers {
            let name = HeaderName::from_bytes(key.as_bytes())
                .map_err(|err| invalid_argument(format!("invalid header name `{key}`: {err}")))?;
            let header_value = HeaderValue::from_str(value).map_err(|err| {
                invalid_argument(format!("invalid header value for `{key}`: {err}"))
            })?;
            map.insert(name, header_value);
        }
        Ok(map)
    }

    fn map_reqwest_error(err: reqwest::Error) -> FunctionsError {
        if err.is_timeout() {
            return FunctionsError::new(
                FunctionsErrorCode::DeadlineExceeded,
                format!("callable request timed out: {err}"),
            );
        }
        if err.is_connect() {
            return FunctionsError::new(
                FunctionsErrorCode::Unavailable,
                format!("failed to connect to callable endpoint: {err}"),
            );
        }
        if err.is_decode() {
            return internal_error(format!("unable to decode callable response: {err}"));
        }
        if err.is_request() {
            return FunctionsError::new(
                FunctionsErrorCode::InvalidArgument,
                format!("malformed callable request: {err}"),
            );
        }
        FunctionsError::new(
            FunctionsErrorCode::Unknown,
            format!("callable request failed: {err}"),
        )
    }

    pub(super) fn invoke(request: CallableRequest) -> FunctionsResult<JsonValue> {
        let CallableRequest {
            url,
            payload,
            timeout,
            headers,
        } = request;

        let header_map = build_headers(&headers)?;
        let response = client()
            .post(url)
            .timeout(timeout)
            .headers(header_map)
            .json(&payload)
            .send()
            .map_err(map_reqwest_error)?;

        handle_response(response)
    }

    fn handle_response(response: Response) -> FunctionsResult<JsonValue> {
        let status = response.status();
        let bytes = response.bytes().map_err(|err| {
            internal_error(format!("failed to read callable response body: {err}"))
        })?;

        let (body, parse_error) = if bytes.is_empty() {
            (None, None)
        } else {
            match serde_json::from_slice::<JsonValue>(&bytes) {
                Ok(value) => (Some(value), None),
                Err(err) => (None, Some(err)),
            }
        };

        if let Some(error) = error_for_http_response(status.as_u16(), body.as_ref()) {
            return Err(error);
        }

        if let Some(err) = parse_error {
            return Err(internal_error(format!(
                "Response is not valid JSON object: {err}"
            )));
        }

        if status == StatusCode::NO_CONTENT {
            return Err(internal_error(
                "Callable response is missing data payload (HTTP 204)",
            ));
        }

        Ok(body.unwrap_or(JsonValue::Null))
    }
}

#[cfg(target_arch = "wasm32")]
struct WasmCallableTransport;

#[cfg(target_arch = "wasm32")]
impl WasmCallableTransport {
    fn new() -> Self {
        Self
    }
}

#[cfg(target_arch = "wasm32")]
impl CallableTransport for WasmCallableTransport {
    fn invoke(&self, _request: CallableRequest) -> FunctionsResult<JsonValue> {
        use crate::functions::error::{FunctionsError, FunctionsErrorCode};

        Err(FunctionsError::new(
            FunctionsErrorCode::Unimplemented,
            "Callable HTTP transport is not yet available for wasm targets",
        ))
    }
}
