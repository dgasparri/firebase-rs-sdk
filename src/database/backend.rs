use std::sync::{Arc, LazyLock, Mutex};

use reqwest::blocking::{Client, Response};
use reqwest::{Method, StatusCode};
use serde_json::{Map, Value};
use url::Url;

use crate::app::FirebaseApp;
use crate::app_check::{FirebaseAppCheckInternal, APP_CHECK_INTERNAL_COMPONENT_NAME};
use crate::auth::Auth;
use crate::database::error::{
    internal_error, invalid_argument, permission_denied, DatabaseError, DatabaseResult,
};
use crate::logger::Logger;
use futures::executor::block_on;

type TokenFetcher = Arc<dyn Fn() -> DatabaseResult<Option<String>> + Send + Sync>;

pub(crate) trait DatabaseBackend: Send + Sync {
    fn set(&self, path: &[String], value: Value) -> DatabaseResult<()>;
    fn update(
        &self,
        base_path: &[String],
        updates: Vec<(Vec<String>, Value)>,
    ) -> DatabaseResult<()>;
    fn delete(&self, path: &[String]) -> DatabaseResult<()>;
    fn get(&self, path: &[String], query: &[(String, String)]) -> DatabaseResult<Value>;
}

pub(crate) fn select_backend(app: &FirebaseApp) -> Arc<dyn DatabaseBackend> {
    let options = app.options();
    if let Some(url) = options.database_url {
        let app_for_auth = app.clone();
        let auth_fetcher: TokenFetcher = Arc::new(move || {
            let container = app_for_auth.container();

            let auth_or_none = container
                .get_provider("auth-internal")
                .get_immediate_with_options::<Auth>(None, true)
                .map_err(|err| internal_error(format!("failed to resolve auth provider: {err}")))?;

            let auth = match auth_or_none {
                Some(auth) => Some(auth),
                None => container
                    .get_provider("auth")
                    .get_immediate_with_options::<Auth>(None, true)
                    .map_err(|err| {
                        internal_error(format!("failed to resolve auth provider: {err}"))
                    })?,
            };

            let Some(auth) = auth else {
                return Ok(None);
            };

            match block_on(auth.get_token(false)) {
                Ok(Some(token)) if token.is_empty() => Ok(None),
                Ok(Some(token)) => Ok(Some(token)),
                Ok(None) => Ok(None),
                Err(err) => Err(internal_error(format!(
                    "failed to obtain auth token: {err}"
                ))),
            }
        });

        let app_for_app_check = app.clone();
        let app_check_fetcher: TokenFetcher = Arc::new(move || {
            let container = app_for_app_check.container();
            let app_check = container
                .get_provider(APP_CHECK_INTERNAL_COMPONENT_NAME)
                .get_immediate_with_options::<FirebaseAppCheckInternal>(None, true)
                .map_err(|err| {
                    internal_error(format!("failed to resolve app check provider: {err}"))
                })?;

            let Some(app_check) = app_check else {
                return Ok(None);
            };

            let result = block_on(app_check.get_token(false)).map_err(|err| {
                internal_error(format!("failed to obtain App Check token: {err}"))
            })?;

            if let Some(error) = result.error.or(result.internal_error) {
                return Err(internal_error(format!("App Check token error: {error}")));
            }

            if result.token.is_empty() {
                Ok(None)
            } else {
                Ok(Some(result.token))
            }
        });

        match RestBackend::new(url, auth_fetcher, app_check_fetcher) {
            Ok(backend) => return Arc::new(backend),
            Err(err) => {
                LOGGER.warn(format!(
                    "Falling back to in-memory Realtime Database backend: {err}"
                ));
            }
        }
    }
    Arc::new(InMemoryBackend::default())
}

struct InMemoryBackend {
    data: Mutex<Value>,
}

impl Default for InMemoryBackend {
    fn default() -> Self {
        Self {
            data: Mutex::new(Value::Object(Default::default())),
        }
    }
}

impl DatabaseBackend for InMemoryBackend {
    fn set(&self, path: &[String], value: Value) -> DatabaseResult<()> {
        let mut data = self.data.lock().unwrap();
        set_at_path(&mut data, path, value);
        Ok(())
    }

    fn update(
        &self,
        _base_path: &[String],
        updates: Vec<(Vec<String>, Value)>,
    ) -> DatabaseResult<()> {
        let mut data = self.data.lock().unwrap();
        for (path, value) in updates {
            set_at_path(&mut data, &path, value);
        }
        Ok(())
    }

    fn delete(&self, path: &[String]) -> DatabaseResult<()> {
        let mut data = self.data.lock().unwrap();
        delete_at_path(&mut data, path);
        Ok(())
    }

    fn get(&self, path: &[String], _query: &[(String, String)]) -> DatabaseResult<Value> {
        let data = self.data.lock().unwrap();
        Ok(get_at_path(&data, path).cloned().unwrap_or(Value::Null))
    }
}

struct RestBackend {
    client: Client,
    base_url: Url,
    base_query: Vec<(String, String)>,
    auth_token_fetcher: TokenFetcher,
    app_check_token_fetcher: TokenFetcher,
}

impl RestBackend {
    fn new(
        raw_url: String,
        auth_token_fetcher: TokenFetcher,
        app_check_token_fetcher: TokenFetcher,
    ) -> DatabaseResult<Self> {
        let mut url = Url::parse(&raw_url)
            .map_err(|err| invalid_argument(format!("Invalid database_url '{raw_url}': {err}")))?;

        // Ensure the base URL ends with a slash so joins behave predictably.
        if !url.path().ends_with('/') {
            let mut path = url.path().trim_end_matches('/').to_owned();
            path.push('/');
            url.set_path(&path);
        }

        let base_query: Vec<(String, String)> = url
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect();
        url.set_query(None);

        let client = Client::builder()
            .build()
            .map_err(|err| internal_error(format!("Failed to build HTTP client: {err}")))?;

        Ok(Self {
            client,
            base_url: url,
            base_query,
            auth_token_fetcher,
            app_check_token_fetcher,
        })
    }

    fn url_for_path(&self, path: &[String], query: &[(String, String)]) -> DatabaseResult<Url> {
        let relative = if path.is_empty() {
            ".json".to_string()
        } else {
            format!("{}.json", path.join("/"))
        };
        let mut url = self
            .base_url
            .join(&relative)
            .map_err(|err| internal_error(format!("Failed to compose database URL: {err}")))?;

        {
            let mut pairs = url.query_pairs_mut();
            pairs.clear();
            for (key, value) in self.base_query.iter().chain(query.iter()) {
                pairs.append_pair(key, value);
            }
        }

        Ok(url)
    }

    fn handle_reqwest_error(&self, err: reqwest::Error) -> DatabaseError {
        if let Some(status) = err.status() {
            return self.handle_http_error(status, None);
        }
        internal_error(format!("Database request failed: {err}"))
    }

    fn handle_http_error(&self, status: StatusCode, body: Option<String>) -> DatabaseError {
        let message = body.as_deref().and_then(extract_error_message);

        match status {
            StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => invalid_argument(
                message
                    .clone()
                    .unwrap_or_else(|| "Invalid data payload".to_string()),
            ),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => permission_denied(
                message
                    .clone()
                    .unwrap_or_else(|| "Permission denied".to_string()),
            ),
            _ => internal_error(format!(
                "Database request failed with status {}{}",
                status.as_str(),
                message
                    .map(|b| format!(": {b}"))
                    .unwrap_or_else(String::new)
            )),
        }
    }

    fn send_request(
        &self,
        method: Method,
        path: &[String],
        query: &[(String, String)],
        body: Option<&Value>,
    ) -> DatabaseResult<Response> {
        let augmented_query = self.query_with_tokens(query)?;
        let url = self.url_for_path(path, &augmented_query)?;
        let mut request = self.client.request(method, url);
        if let Some(payload) = body {
            request = request.json(payload);
        }

        request.send().map_err(|err| self.handle_reqwest_error(err))
    }

    fn ensure_success(&self, response: Response) -> DatabaseResult<Response> {
        if response.status().is_success() {
            Ok(response)
        } else {
            let status = response.status();
            let body = response.text().ok();
            Err(self.handle_http_error(status, body))
        }
    }

    fn query_with_tokens(
        &self,
        query: &[(String, String)],
    ) -> DatabaseResult<Vec<(String, String)>> {
        let mut params: Vec<(String, String)> = query.to_vec();

        if !params.iter().any(|(key, _)| key == "auth") {
            if let Some(token) = fetch_token(&self.auth_token_fetcher)? {
                params.push(("auth".to_string(), token));
            }
        }

        if !params.iter().any(|(key, _)| key == "ac") {
            if let Some(token) = fetch_token(&self.app_check_token_fetcher)? {
                params.push(("ac".to_string(), token));
            }
        }

        Ok(params)
    }
}

fn fetch_token(fetcher: &TokenFetcher) -> DatabaseResult<Option<String>> {
    (fetcher.as_ref())()
}

impl DatabaseBackend for RestBackend {
    fn set(&self, path: &[String], value: Value) -> DatabaseResult<()> {
        let mut params = Vec::with_capacity(1);
        params.push(("print".to_string(), "silent".to_string()));
        let response = self.send_request(Method::PUT, path, &params, Some(&value))?;
        self.ensure_success(response).map(|_| ())
    }

    fn update(
        &self,
        base_path: &[String],
        updates: Vec<(Vec<String>, Value)>,
    ) -> DatabaseResult<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let mut payload = Map::with_capacity(updates.len());
        for (absolute_path, value) in updates {
            if !path_starts_with(&absolute_path, base_path) {
                return Err(internal_error(
                    "Database update contained a path outside the reference",
                ));
            }
            let relative = &absolute_path[base_path.len()..];
            if relative.is_empty() {
                return Err(invalid_argument(
                    "Database update path cannot be empty relative to the reference",
                ));
            }
            payload.insert(relative.join("/"), value);
        }

        let body = Value::Object(payload);
        let mut params = Vec::with_capacity(1);
        params.push(("print".to_string(), "silent".to_string()));
        let response = self.send_request(Method::PATCH, base_path, &params, Some(&body))?;
        self.ensure_success(response).map(|_| ())
    }

    fn delete(&self, path: &[String]) -> DatabaseResult<()> {
        let mut params = Vec::with_capacity(1);
        params.push(("print".to_string(), "silent".to_string()));
        let response = self.send_request(Method::DELETE, path, &params, None)?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        self.ensure_success(response).map(|_| ())
    }

    fn get(&self, path: &[String], query: &[(String, String)]) -> DatabaseResult<Value> {
        let mut params = Vec::with_capacity(query.len() + 1);
        if !query.iter().any(|(key, _)| key == "format") {
            params.push(("format".to_string(), "export".to_string()));
        }
        params.extend_from_slice(query);

        let response = self.send_request(Method::GET, path, &params, None)?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(Value::Null);
        }

        let response = self.ensure_success(response)?;

        response
            .json()
            .map_err(|err| internal_error(format!("Failed to decode database response: {err}")))
    }
}

fn set_at_path(root: &mut Value, path: &[String], value: Value) {
    if path.is_empty() {
        *root = value;
        return;
    }

    let mut current = root;
    for segment in &path[..path.len() - 1] {
        if !current.is_object() {
            *current = Value::Object(Default::default());
        }
        let obj = current.as_object_mut().unwrap();
        current = obj
            .entry(segment)
            .or_insert(Value::Object(Default::default()));
    }

    if !current.is_object() {
        *current = Value::Object(Default::default());
    }
    current
        .as_object_mut()
        .unwrap()
        .insert(path.last().unwrap().clone(), value);
}

fn get_at_path<'a>(root: &'a Value, path: &[String]) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(root);
    }
    let mut current = root;
    for segment in path {
        match current {
            Value::Object(obj) => match obj.get(segment) {
                Some(value) => current = value,
                None => return None,
            },
            _ => return None,
        }
    }
    Some(current)
}

fn path_starts_with(path: &[String], prefix: &[String]) -> bool {
    if prefix.len() > path.len() {
        return false;
    }
    path.iter()
        .zip(prefix.iter())
        .all(|(left, right)| left == right)
}

fn delete_at_path(root: &mut Value, path: &[String]) {
    if path.is_empty() {
        *root = Value::Null;
        return;
    }

    let mut current = root;
    for segment in &path[..path.len() - 1] {
        match current {
            Value::Object(obj) => match obj.get_mut(segment) {
                Some(next) => {
                    current = next;
                }
                None => return,
            },
            _ => return,
        }
    }

    if let Value::Object(obj) = current {
        obj.remove(path.last().unwrap());
    }
}

fn extract_error_message(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }

    if let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(raw) {
        if let Some(Value::String(message)) = obj.get("error") {
            return Some(message.clone());
        }
    }

    Some(raw.to_string())
}

static LOGGER: LazyLock<Logger> = LazyLock::new(|| Logger::new("@firebase/database"));

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use serde_json::json;

    fn static_token(value: &'static str) -> TokenFetcher {
        Arc::new(move || Ok(Some(value.to_string())))
    }

    fn empty_token() -> TokenFetcher {
        Arc::new(|| Ok(None))
    }

    #[test]
    fn rest_backend_attaches_tokens_to_requests() {
        let server = MockServer::start();

        let get_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/items.json")
                .query_param("auth", "id-token")
                .query_param("ac", "app-check")
                .query_param("format", "export");
            then.status(200).body("null");
        });

        let backend = RestBackend::new(
            server.url("/"),
            static_token("id-token"),
            static_token("app-check"),
        )
        .expect("rest backend");

        backend.get(&["items".to_string()], &[]).unwrap();

        get_mock.assert();
    }

    #[test]
    fn rest_backend_skips_missing_tokens() {
        let server = MockServer::start();

        let put_mock = server.mock(|when, then| {
            when.method(PUT)
                .path("/data.json")
                .query_param("print", "silent")
                .json_body(json!({"value": true}));
            then.status(200).body("null");
        });

        let backend = RestBackend::new(server.url("/"), empty_token(), empty_token()).unwrap();

        backend
            .set(&["data".to_string()], json!({"value": true}))
            .unwrap();

        put_mock.assert();
    }
}
