use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use reqwest::Url;
use serde::Deserialize;
use serde_json::Value;

use crate::data_connect::config::{DataConnectOptions, TransportOptions};
use crate::data_connect::error::{
    internal_error, operation_error, unauthorized, DataConnectErrorPathSegment,
    DataConnectOperationFailureResponse, DataConnectOperationFailureResponseErrorInfo,
    DataConnectResult,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallerSdkType {
    Base,
    Generated,
    TanstackReactCore,
    GeneratedReact,
    TanstackAngularCore,
    GeneratedAngular,
}

impl Default for CallerSdkType {
    fn default() -> Self {
        CallerSdkType::Base
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait RequestTokenProvider: Send + Sync {
    async fn auth_token(&self) -> DataConnectResult<Option<String>>;
    async fn app_check_headers(&self) -> DataConnectResult<Option<AppCheckHeaders>>;
}

#[derive(Clone, Debug, Default)]
pub struct AppCheckHeaders {
    pub token: String,
    pub heartbeat: Option<String>,
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait DataConnectTransport: Send + Sync {
    async fn invoke_query(&self, operation: &str, variables: &Value) -> DataConnectResult<Value>;
    async fn invoke_mutation(&self, operation: &str, variables: &Value)
        -> DataConnectResult<Value>;
    fn use_emulator(&self, options: TransportOptions);
    fn set_generated_sdk(&self, enabled: bool);
    fn set_caller_sdk_type(&self, caller: CallerSdkType);
}

pub struct RestTransport {
    client: reqwest::Client,
    options: DataConnectOptions,
    api_key: Option<String>,
    app_id: Option<String>,
    token_provider: Arc<dyn RequestTokenProvider>,
    state: Mutex<TransportState>,
    generated_sdk: AtomicBool,
    caller_sdk_type: Mutex<CallerSdkType>,
}

struct TransportState {
    transport: TransportOptions,
    is_emulator: bool,
}

impl RestTransport {
    pub fn new(
        options: DataConnectOptions,
        api_key: Option<String>,
        app_id: Option<String>,
        token_provider: Arc<dyn RequestTokenProvider>,
    ) -> DataConnectResult<Self> {
        Ok(Self {
            client: reqwest::Client::new(),
            options,
            api_key,
            app_id,
            token_provider,
            state: Mutex::new(TransportState {
                transport: TransportOptions::default(),
                is_emulator: false,
            }),
            generated_sdk: AtomicBool::new(false),
            caller_sdk_type: Mutex::new(CallerSdkType::Base),
        })
    }

    fn endpoint_url(&self, action: &str) -> DataConnectResult<Url> {
        let state = self.state.lock().unwrap();
        let base = state.transport.base_url();
        let path = format!("{base}/v1/{}:{}", self.options.resource_path(), action);
        let mut url = Url::parse(&path).map_err(|err| internal_error(err.to_string()))?;
        if let Some(key) = &self.api_key {
            url.query_pairs_mut().append_pair("key", key);
        }
        Ok(url)
    }

    fn goog_api_client_header(&self) -> String {
        let sdk_version = env!("CARGO_PKG_VERSION");
        let mut header = format!("gl-rs/ fire/{sdk_version}");
        if self.generated_sdk.load(Ordering::SeqCst) {
            header.push_str(" rs/gen");
        }
        match &*self.caller_sdk_type.lock().unwrap() {
            CallerSdkType::Base => {}
            CallerSdkType::Generated => header.push_str(" js/gen"),
            CallerSdkType::TanstackReactCore => header.push_str(" js/tanstack-react"),
            CallerSdkType::GeneratedReact => header.push_str(" js/gen-react"),
            CallerSdkType::TanstackAngularCore => header.push_str(" js/tanstack-angular"),
            CallerSdkType::GeneratedAngular => header.push_str(" js/gen-angular"),
        }
        header
    }

    async fn perform_request(
        &self,
        action: &str,
        operation: &str,
        variables: &Value,
    ) -> DataConnectResult<Value> {
        let mut body = serde_json::Map::new();
        body.insert(
            "name".to_string(),
            Value::String(format!(
                "projects/{}/locations/{}/services/{}/connectors/{}",
                self.options.project_id,
                self.options.connector.location,
                self.options.connector.service,
                self.options.connector.connector,
            )),
        );
        body.insert(
            "operationName".to_string(),
            Value::String(operation.to_string()),
        );
        body.insert("variables".to_string(), variables.clone());

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "X-Goog-Api-Client",
            HeaderValue::from_str(&self.goog_api_client_header())
                .map_err(|err| internal_error(err.to_string()))?,
        );

        if let Some(app_id) = &self.app_id {
            if !app_id.is_empty() {
                headers.insert(
                    "X-Firebase-GMPID",
                    HeaderValue::from_str(app_id).map_err(|err| internal_error(err.to_string()))?,
                );
            }
        }

        if let Some(token) = self.token_provider.auth_token().await? {
            if !token.is_empty() {
                headers.insert(
                    "X-Firebase-Auth-Token",
                    HeaderValue::from_str(&token).map_err(|err| internal_error(err.to_string()))?,
                );
            }
        }

        if let Some(app_check) = self.token_provider.app_check_headers().await? {
            if !app_check.token.is_empty() {
                headers.insert(
                    "X-Firebase-AppCheck",
                    HeaderValue::from_str(&app_check.token)
                        .map_err(|err| internal_error(err.to_string()))?,
                );
            }
            if let Some(heartbeat) = &app_check.heartbeat {
                if !heartbeat.is_empty() {
                    headers.insert(
                        "X-Firebase-Client",
                        HeaderValue::from_str(heartbeat)
                            .map_err(|err| internal_error(err.to_string()))?,
                    );
                }
            }
        }

        let url = self.endpoint_url(action)?;
        let response = self
            .client
            .post(url)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(|err| internal_error(err.to_string()))?;

        if response.status().as_u16() == 401 {
            return Err(unauthorized("Request unauthorized"));
        }
        if !response.status().is_success() {
            return Err(internal_error(format!(
                "Data Connect request failed with status {}",
                response.status()
            )));
        }

        let graph_response: GraphQlResponse = response
            .json()
            .await
            .map_err(|err| internal_error(err.to_string()))?;
        if !graph_response.errors.is_empty() {
            let response = DataConnectOperationFailureResponse {
                data: graph_response.data,
                errors: graph_response
                    .errors
                    .into_iter()
                    .map(|error| DataConnectOperationFailureResponseErrorInfo {
                        message: error
                            .message
                            .unwrap_or_else(|| "Unknown Data Connect error".to_string()),
                        path: error
                            .path
                            .unwrap_or_default()
                            .into_iter()
                            .filter_map(|segment| match segment {
                                Value::String(field) => {
                                    Some(DataConnectErrorPathSegment::Field(field))
                                }
                                Value::Number(num) => num
                                    .as_i64()
                                    .map(|idx| DataConnectErrorPathSegment::Index(idx)),
                                _ => None,
                            })
                            .collect(),
                    })
                    .collect(),
            };
            return Err(operation_error(
                format!("Data Connect error executing {operation}"),
                response,
            ));
        }

        Ok(graph_response.data.unwrap_or(Value::Null))
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl DataConnectTransport for RestTransport {
    async fn invoke_query(&self, operation: &str, variables: &Value) -> DataConnectResult<Value> {
        self.perform_request("executeQuery", operation, variables)
            .await
    }

    async fn invoke_mutation(
        &self,
        operation: &str,
        variables: &Value,
    ) -> DataConnectResult<Value> {
        self.perform_request("executeMutation", operation, variables)
            .await
    }

    fn use_emulator(&self, options: TransportOptions) {
        let mut state = self.state.lock().unwrap();
        state.transport = options;
        state.is_emulator = true;
    }

    fn set_generated_sdk(&self, enabled: bool) {
        self.generated_sdk.store(enabled, Ordering::SeqCst);
    }

    fn set_caller_sdk_type(&self, caller: CallerSdkType) {
        *self.caller_sdk_type.lock().unwrap() = caller;
    }
}

#[derive(Deserialize)]
struct GraphQlResponse {
    #[serde(default)]
    data: Option<Value>,
    #[serde(default)]
    errors: Vec<GraphQlError>,
}

#[derive(Deserialize)]
struct GraphQlError {
    message: Option<String>,
    path: Option<Vec<Value>>,
}
