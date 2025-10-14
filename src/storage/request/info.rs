use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Method;

use crate::storage::error::{StorageError, StorageResult};

use super::transport::ResponsePayload;

pub type ResponseHandler<O> = Arc<dyn Fn(ResponsePayload) -> StorageResult<O> + Send + Sync>;
pub type ErrorHandler = Arc<dyn Fn(ResponsePayload, StorageError) -> StorageError + Send + Sync>;

#[derive(Clone, Debug)]
pub enum RequestBody {
    Bytes(Vec<u8>),
    Text(String),
    Empty,
}

impl RequestBody {
    pub fn is_empty(&self) -> bool {
        matches!(self, RequestBody::Empty)
    }
}

pub struct RequestInfo<O> {
    pub url: String,
    pub method: Method,
    pub headers: HashMap<String, String>,
    pub body: RequestBody,
    pub success_codes: Vec<u16>,
    pub additional_retry_codes: Vec<u16>,
    pub timeout: Duration,
    pub response_handler: ResponseHandler<O>,
    pub error_handler: Option<ErrorHandler>,
}

impl<O> Clone for RequestInfo<O> {
    fn clone(&self) -> Self {
        Self {
            url: self.url.clone(),
            method: self.method.clone(),
            headers: self.headers.clone(),
            body: self.body.clone(),
            success_codes: self.success_codes.clone(),
            additional_retry_codes: self.additional_retry_codes.clone(),
            timeout: self.timeout,
            response_handler: Arc::clone(&self.response_handler),
            error_handler: self.error_handler.as_ref().map(Arc::clone),
        }
    }
}

impl<O> RequestInfo<O> {
    pub fn new(
        url: impl Into<String>,
        method: Method,
        timeout: Duration,
        response_handler: ResponseHandler<O>,
    ) -> Self {
        Self {
            url: url.into(),
            method,
            headers: HashMap::new(),
            body: RequestBody::Empty,
            success_codes: vec![200],
            additional_retry_codes: Vec::new(),
            timeout,
            response_handler,
            error_handler: None,
        }
    }

    pub fn with_body(mut self, body: RequestBody) -> Self {
        self.body = body;
        self
    }

    pub fn with_headers(mut self, headers: HashMap<String, String>) -> Self {
        self.headers = headers;
        self
    }

    pub fn with_success_codes(mut self, codes: Vec<u16>) -> Self {
        self.success_codes = codes;
        self
    }

    pub fn with_additional_retry_codes(mut self, codes: Vec<u16>) -> Self {
        self.additional_retry_codes = codes;
        self
    }

    pub fn with_error_handler(mut self, handler: ErrorHandler) -> Self {
        self.error_handler = Some(handler);
        self
    }
}
