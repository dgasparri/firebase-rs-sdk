use crate::storage::error::{internal_error, StorageError, StorageResult};
use crate::storage::util::is_url;
use reqwest::blocking::{Client, Response};
use reqwest::{StatusCode, Url};
use std::collections::HashMap;
use std::time::Duration;

use super::backoff::{BackoffConfig, BackoffState};
use super::info::{RequestBody, RequestInfo};

#[derive(Clone, Debug)]
pub struct ResponsePayload {
    pub status: StatusCode,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl ResponsePayload {
    fn from_response(response: Response) -> StorageResult<Self> {
        let status = response.status();
        let mut headers = HashMap::new();
        for (key, value) in response.headers().iter() {
            if let Ok(val) = value.to_str() {
                headers.insert(key.as_str().to_owned(), val.to_owned());
            }
        }
        let body = response
            .bytes()
            .map_err(|err| internal_error(format!("failed to read response body: {err}")))?
            .to_vec();
        Ok(Self {
            status,
            headers,
            body,
        })
    }
}

#[derive(Debug)]
pub enum RequestError {
    Network(String),
    Timeout,
    Fatal(StorageError),
}

#[derive(Clone)]
pub struct HttpClient {
    client: Client,
    is_using_emulator: bool,
    backoff: BackoffConfig,
}

impl HttpClient {
    pub fn new(is_using_emulator: bool, backoff: BackoffConfig) -> StorageResult<Self> {
        let client = Client::builder()
            .build()
            .map_err(|err| internal_error(format!("failed to build HTTP client: {err}")))?;
        Ok(Self {
            client,
            is_using_emulator,
            backoff,
        })
    }

    pub fn execute<O>(&self, info: RequestInfo<O>) -> StorageResult<O> {
        let mut backoff = BackoffState::new(self.backoff.clone());

        loop {
            if !backoff.has_time_remaining() {
                return Err(internal_error("storage request timed out"));
            }

            let delay = backoff.next_delay();
            if delay > Duration::from_millis(0) {
                std::thread::sleep(delay);
            }

            let result = self.try_once(&info);

            match result {
                Ok(payload) => {
                    if info.success_codes.contains(&payload.status.as_u16()) {
                        return (info.response_handler)(payload);
                    }

                    if should_retry(payload.status, &info) && backoff.can_retry() {
                        continue;
                    }

                    return Err(map_failure(payload, &info));
                }
                Err(RequestError::Fatal(err)) => return Err(err),
                Err(RequestError::Timeout) => {
                    return Err(internal_error("storage request timed out"));
                }
                Err(RequestError::Network(reason)) => {
                    if backoff.can_retry() {
                        continue;
                    }
                    return Err(internal_error(format!(
                        "network failure after retries: {reason}"
                    )));
                }
            }
        }
    }

    fn try_once<O>(&self, info: &RequestInfo<O>) -> Result<ResponsePayload, RequestError> {
        let url = self.prepare_url(&info.url).map_err(RequestError::Fatal)?;

        let mut request_builder = self.client.request(info.method.clone(), url);
        request_builder = request_builder.timeout(info.timeout);

        for (header, value) in &info.headers {
            request_builder = request_builder.header(header, value);
        }

        match &info.body {
            RequestBody::Bytes(bytes) => {
                if !bytes.is_empty() {
                    request_builder = request_builder.body(bytes.clone());
                }
            }
            RequestBody::Text(text) => {
                if !text.is_empty() {
                    request_builder = request_builder.body(text.clone());
                }
            }
            RequestBody::Empty => {}
        }

        let response = request_builder.send().map_err(|err| {
            if err.is_timeout() {
                RequestError::Timeout
            } else {
                RequestError::Network(err.to_string())
            }
        })?;

        ResponsePayload::from_response(response).map_err(RequestError::Fatal)
    }

    fn prepare_url(&self, raw: &str) -> StorageResult<Url> {
        if is_url(raw) {
            Url::parse(raw).map_err(|err| internal_error(format!("invalid storage URL: {err}")))
        } else {
            let scheme = if self.is_using_emulator {
                "http"
            } else {
                "https"
            };
            let formatted = format!("{scheme}://{raw}");
            Url::parse(&formatted)
                .map_err(|err| internal_error(format!("invalid storage URL: {err}")))
        }
    }
}

fn should_retry<O>(status: StatusCode, info: &RequestInfo<O>) -> bool {
    crate::storage::util::is_retry_status_code(status.as_u16(), &info.additional_retry_codes)
}

fn map_failure<O>(payload: ResponsePayload, info: &RequestInfo<O>) -> StorageError {
    let base_error = internal_error(format!(
        "storage request failed with status {}",
        payload.status
    ))
    .with_status(payload.status.as_u16())
    .with_server_response(String::from_utf8_lossy(&payload.body).to_string());

    if let Some(handler) = &info.error_handler {
        handler(payload, base_error)
    } else {
        base_error
    }
}
