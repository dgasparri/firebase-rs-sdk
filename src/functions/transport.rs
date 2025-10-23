use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

use async_trait::async_trait;
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

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait CallableTransport: Send + Sync {
    async fn invoke(&self, request: CallableRequest) -> FunctionsResult<JsonValue>;
}

pub async fn invoke_callable_async(request: CallableRequest) -> FunctionsResult<JsonValue> {
    callable_transport().invoke(request).await
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
#[async_trait]
impl CallableTransport for NativeCallableTransport {
    async fn invoke(&self, request: CallableRequest) -> FunctionsResult<JsonValue> {
        native::invoke(request).await
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{CallableRequest, JsonValue};
    use crate::functions::error::{
        error_for_http_response, internal_error, invalid_argument, FunctionsError,
        FunctionsErrorCode, FunctionsResult,
    };
    use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
    use reqwest::StatusCode;
    use reqwest::{Client, Response};
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

    pub(super) async fn invoke(request: CallableRequest) -> FunctionsResult<JsonValue> {
        let CallableRequest {
            url,
            payload,
            timeout,
            headers,
        } = request;

        let header_map = build_headers(&headers)?;
        let client = client().clone();

        let response = client
            .post(url)
            .timeout(timeout)
            .headers(header_map)
            .json(&payload)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        handle_response(response).await
    }

    async fn handle_response(response: Response) -> FunctionsResult<JsonValue> {
        let status = response.status();
        let bytes = response.bytes().await.map_err(|err| {
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
#[async_trait(?Send)]
impl CallableTransport for WasmCallableTransport {
    async fn invoke(&self, _request: CallableRequest) -> FunctionsResult<JsonValue> {
        wasm::invoke(_request).await
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::{CallableRequest, JsonValue};
    use crate::functions::error::{
        error_for_http_response, internal_error, invalid_argument, FunctionsError,
        FunctionsErrorCode, FunctionsResult,
    };
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{AbortController, DomException, Request, RequestInit, RequestMode, Response};

    const TIMEOUT_CONTEXT: &str = "callable request timed out";

    pub(super) async fn invoke(request: CallableRequest) -> FunctionsResult<JsonValue> {
        let CallableRequest {
            url,
            payload,
            timeout,
            headers,
        } = request;

        let window = web_sys::window()
            .ok_or_else(|| internal_error("window is not available in this environment"))?;

        let abort_controller = AbortController::new()
            .map_err(|err| internal_error(format_js_error("create AbortController", err)))?;
        let signal = abort_controller.signal();

        let mut init = RequestInit::new();
        init.method("POST");
        init.mode(RequestMode::Cors);
        init.signal(Some(&signal));

        let body = serde_json::to_string(&payload).map_err(|err| {
            internal_error(format!("Failed to serialize callable payload: {err}"))
        })?;
        init.body(Some(&JsValue::from_str(&body)));

        let request = Request::new_with_str_and_init(&url, &init)
            .map_err(|err| internal_error(format_js_error("build callable request", err)))?;

        let request_headers = request.headers();
        for (key, value) in headers {
            request_headers.set(&key, &value).map_err(|err| {
                invalid_argument(format!(
                    "invalid header `{key}`: {}",
                    format_js_error("set header", err)
                ))
            })?;
        }

        let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as i32;

        let mut timeout_guard = if timeout_ms > 0 {
            let controller = abort_controller.clone();
            let closure = Closure::wrap(Box::new(move || {
                controller.abort();
            }) as Box<dyn FnMut()>);
            let handle = window
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    closure.as_ref().unchecked_ref(),
                    timeout_ms,
                )
                .map_err(|err| internal_error(format_js_error("schedule callable timeout", err)))?;
            Some((handle, closure))
        } else {
            None
        };

        let response_value = match JsFuture::from(window.fetch_with_request(&request)).await {
            Ok(value) => value,
            Err(err) => {
                cancel_timeout(&window, &mut timeout_guard);
                if is_abort_error(&err) {
                    return Err(FunctionsError::new(
                        FunctionsErrorCode::DeadlineExceeded,
                        TIMEOUT_CONTEXT,
                    ));
                }
                return Err(internal_error(format_js_error("callable fetch", err)));
            }
        };

        cancel_timeout(&window, &mut timeout_guard);

        let response: Response = response_value
            .dyn_into()
            .map_err(|_| internal_error("callable fetch did not return a Response"))?;

        let status = response.status();
        let text_promise = response
            .text()
            .map_err(|err| internal_error(format_js_error("read callable response", err)))?;
        let text_value = JsFuture::from(text_promise)
            .await
            .map_err(|err| internal_error(format_js_error("resolve callable response", err)))?;

        let text = text_value.as_string().unwrap_or_default();
        let (body, parse_error) = if text.trim().is_empty() {
            (None, None)
        } else {
            match serde_json::from_str::<JsonValue>(&text) {
                Ok(value) => (Some(value), None),
                Err(err) => (None, Some(err)),
            }
        };

        if let Some(error) = error_for_http_response(status, body.as_ref()) {
            return Err(error);
        }

        if let Some(err) = parse_error {
            return Err(internal_error(format!(
                "Response is not valid JSON object: {err}"
            )));
        }

        if status == 204 {
            return Err(internal_error(
                "Callable response is missing data payload (HTTP 204)",
            ));
        }

        Ok(body.unwrap_or(JsonValue::Null))
    }

    fn is_abort_error(err: &JsValue) -> bool {
        if let Some(dom_error) = err.dyn_ref::<DomException>() {
            return dom_error.name() == "AbortError";
        }
        if let Some(string) = err.as_string() {
            return string.contains("AbortError");
        }
        false
    }

    fn cancel_timeout(window: &web_sys::Window, guard: &mut Option<(i32, Closure<dyn FnMut()>)>) {
        if let Some((handle, _)) = guard {
            window.clear_timeout_with_handle(*handle);
        }
        guard.take();
    }

    fn format_js_error(context: &str, err: JsValue) -> String {
        let description = if let Some(string) = err.as_string() {
            string
        } else if let Some(dom_error) = err.dyn_ref::<DomException>() {
            format!("{}: {}", dom_error.name(), dom_error.message())
        } else {
            format!("{:?}", err)
        };
        format!("{context}: {description}")
    }
}
