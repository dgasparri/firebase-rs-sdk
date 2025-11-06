use std::time::Duration;

use reqwest::{Client, Method, RequestBuilder, StatusCode};
use serde_json::Value as JsonValue;

use crate::firestore::error::{internal_error, FirestoreResult};
use crate::firestore::model::database_id::DatabaseId;

use super::rpc_error::map_http_error;

const FIRESTORE_API_HOST: &str = "https://firestore.googleapis.com";
const FIRESTORE_API_VERSION: &str = "v1";

#[derive(Clone, Debug)]
pub struct Connection {
    client: Client,
    base_url: String,
}

#[derive(Clone, Debug)]
pub struct ConnectionBuilder {
    database_id: DatabaseId,
    client: Option<Client>,
    emulator_host: Option<String>,
}

#[derive(Default, Clone, Debug)]
pub struct RequestContext {
    pub auth_token: Option<String>,
    pub app_check_token: Option<String>,
    pub heartbeat_header: Option<String>,
    pub request_timeout: Option<Duration>,
}

impl ConnectionBuilder {
    pub fn new(database_id: DatabaseId) -> Self {
        Self {
            database_id,
            client: None,
            emulator_host: std::env::var("FIRESTORE_EMULATOR_HOST").ok(),
        }
    }

    pub fn with_client(mut self, client: Client) -> Self {
        self.client = Some(client);
        self
    }

    pub fn with_emulator_host(mut self, host: impl Into<String>) -> Self {
        self.emulator_host = Some(host.into());
        self
    }

    pub fn build(self) -> FirestoreResult<Connection> {
        let client = match self.client {
            Some(client) => client,
            None => Client::builder()
                .build()
                .map_err(|err| internal_error(err.to_string()))?,
        };
        let base_url = build_base_url(&self.database_id, self.emulator_host.as_deref());
        Ok(Connection { client, base_url })
    }
}

impl Connection {
    pub fn builder(database_id: DatabaseId) -> ConnectionBuilder {
        ConnectionBuilder::new(database_id)
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn invoke_json(
        &self,
        method: Method,
        path: &str,
        body: Option<JsonValue>,
        context: &RequestContext,
    ) -> FirestoreResult<JsonValue> {
        self.invoke_json_owned(method, path.to_owned(), body, context.clone())
            .await
    }

    pub async fn invoke_json_optional(
        &self,
        method: Method,
        path: &str,
        body: Option<JsonValue>,
        context: &RequestContext,
    ) -> FirestoreResult<Option<JsonValue>> {
        self.invoke_json_optional_owned(method, path.to_owned(), body, context.clone())
            .await
    }

    async fn invoke_json_owned(
        &self,
        method: Method,
        path: String,
        body: Option<JsonValue>,
        context: RequestContext,
    ) -> FirestoreResult<JsonValue> {
        let mut request = self.build_request(method, &path, &context);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|err| internal_error(err.to_string()))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|err| internal_error(err.to_string()))?;
        if status.is_success() {
            if text.is_empty() {
                Ok(JsonValue::Null)
            } else {
                serde_json::from_str(&text).map_err(|err| internal_error(err.to_string()))
            }
        } else {
            Err(map_http_error(status, &text))
        }
    }

    async fn invoke_json_optional_owned(
        &self,
        method: Method,
        path: String,
        body: Option<JsonValue>,
        context: RequestContext,
    ) -> FirestoreResult<Option<JsonValue>> {
        let mut request = self.build_request(method, &path, &context);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|err| internal_error(err.to_string()))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|err| internal_error(err.to_string()))?;
        if status.is_success() {
            if text.is_empty() {
                Ok(Some(JsonValue::Null))
            } else {
                serde_json::from_str(&text)
                    .map(Some)
                    .map_err(|err| internal_error(err.to_string()))
            }
        } else if status == StatusCode::NOT_FOUND {
            Ok(None)
        } else {
            Err(map_http_error(status, &text))
        }
    }

    fn build_request(
        &self,
        method: Method,
        path: &str,
        context: &RequestContext,
    ) -> RequestBuilder {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));
        let mut builder = self.client.request(method, url);
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(timeout) = context.request_timeout {
                builder = builder.timeout(timeout);
            }
        }
        if let Some(token) = context.auth_token.as_deref() {
            builder = builder.bearer_auth(token);
        }
        if let Some(app_check) = context.app_check_token.as_deref() {
            builder = builder.header("X-Firebase-AppCheck", app_check);
        }
        if let Some(header) = context.heartbeat_header.as_deref() {
            builder = builder.header("X-Firebase-Client", header);
        }
        builder = builder.header("Content-Type", "application/json");
        builder
    }
}

fn build_base_url(database_id: &DatabaseId, emulator_host: Option<&str>) -> String {
    match emulator_host {
        Some(host) => format!(
            "http://{host}/{api_version}/projects/{}/databases/{}",
            database_id.project_id(),
            database_id.database(),
            api_version = FIRESTORE_API_VERSION
        ),
        None => format!(
            "{host}/{api_version}/projects/{}/databases/{}",
            database_id.project_id(),
            database_id.database(),
            host = FIRESTORE_API_HOST,
            api_version = FIRESTORE_API_VERSION
        ),
    }
}
