use std::sync::{Arc, LazyLock, Mutex};

use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde_json::Value;
use url::Url;

use crate::app::FirebaseApp;
use crate::database::error::{
    internal_error, invalid_argument, permission_denied, DatabaseError, DatabaseResult,
};
use crate::logger::Logger;

pub(crate) trait DatabaseBackend: Send + Sync {
    fn set(&self, path: &[String], value: Value) -> DatabaseResult<()>;
    fn get(&self, path: &[String]) -> DatabaseResult<Value>;
}

pub(crate) fn select_backend(app: &FirebaseApp) -> Arc<dyn DatabaseBackend> {
    let options = app.options();
    if let Some(url) = options.database_url {
        match RestBackend::new(url) {
            Ok(backend) => return Arc::new(backend),
            Err(err) => {
                LOGGER.warn(format!(
                    "Falling back to in-memory Realtime Database backend: {}",
                    err
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
        set_at_path(&mut *data, path, value);
        Ok(())
    }

    fn get(&self, path: &[String]) -> DatabaseResult<Value> {
        let data = self.data.lock().unwrap();
        Ok(get_at_path(&*data, path).cloned().unwrap_or(Value::Null))
    }
}

struct RestBackend {
    client: Client,
    base_url: Url,
}

impl RestBackend {
    fn new(raw_url: String) -> DatabaseResult<Self> {
        let mut url = Url::parse(&raw_url)
            .map_err(|err| invalid_argument(format!("Invalid database_url '{raw_url}': {err}")))?;

        // Ensure the base URL ends with a slash so joins behave predictably.
        if !url.path().ends_with('/') {
            let mut path = url.path().trim_end_matches('/').to_owned();
            path.push('/');
            url.set_path(&path);
        }

        let client = Client::builder()
            .build()
            .map_err(|err| internal_error(format!("Failed to build HTTP client: {err}")))?;

        Ok(Self {
            client,
            base_url: url,
        })
    }

    fn url_for_path(&self, path: &[String]) -> DatabaseResult<Url> {
        let relative = if path.is_empty() {
            ".json".to_string()
        } else {
            format!("{}.json", path.join("/"))
        };
        self.base_url
            .join(&relative)
            .map_err(|err| internal_error(format!("Failed to compose database URL: {err}")))
    }

    fn handle_reqwest_error(&self, err: reqwest::Error) -> DatabaseError {
        if let Some(status) = err.status() {
            return self.handle_http_error(status, None);
        }
        internal_error(format!("Database request failed: {err}"))
    }

    fn handle_http_error(&self, status: StatusCode, body: Option<String>) -> DatabaseError {
        match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                permission_denied(body.unwrap_or_else(|| "Permission denied".to_string()))
            }
            _ => internal_error(format!(
                "Database request failed with status {}{}",
                status.as_str(),
                body.map(|b| format!(": {b}")).unwrap_or_default()
            )),
        }
    }
}

impl DatabaseBackend for RestBackend {
    fn set(&self, path: &[String], value: Value) -> DatabaseResult<()> {
        let url = self.url_for_path(path)?;
        let response = self
            .client
            .put(url)
            .json(&value)
            .send()
            .map_err(|err| self.handle_reqwest_error(err))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().ok();
            return Err(self.handle_http_error(status, body));
        }

        Ok(())
    }

    fn get(&self, path: &[String]) -> DatabaseResult<Value> {
        let url = self.url_for_path(path)?;
        let response = self
            .client
            .get(url)
            .send()
            .map_err(|err| self.handle_reqwest_error(err))?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(Value::Null);
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().ok();
            return Err(self.handle_http_error(status, body));
        }

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

static LOGGER: LazyLock<Logger> = LazyLock::new(|| Logger::new("@firebase/database"));
