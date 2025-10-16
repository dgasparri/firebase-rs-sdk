use std::fmt;
use std::sync::{Arc, LazyLock};

use serde_json::Value;

use crate::app;
use crate::app::FirebaseApp;
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType};
use crate::database::backend::{select_backend, DatabaseBackend};
use crate::database::constants::DATABASE_COMPONENT_NAME;
use crate::database::error::{internal_error, invalid_argument, DatabaseResult};

#[derive(Clone, Debug)]
pub struct Database {
    inner: Arc<DatabaseInner>,
}

struct DatabaseInner {
    app: FirebaseApp,
    backend: Arc<dyn DatabaseBackend>,
}

impl fmt::Debug for DatabaseInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DatabaseInner")
            .field("app", &self.app.name())
            .field("backend", &"dynamic")
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct DatabaseReference {
    database: Database,
    path: Vec<String>,
}

impl Database {
    fn new(app: FirebaseApp) -> Self {
        Self {
            inner: Arc::new(DatabaseInner {
                backend: select_backend(&app),
                app,
            }),
        }
    }

    pub fn app(&self) -> &FirebaseApp {
        &self.inner.app
    }

    pub fn reference(&self, path: &str) -> DatabaseResult<DatabaseReference> {
        let segments = normalize_path(path)?;
        Ok(DatabaseReference {
            database: self.clone(),
            path: segments,
        })
    }
}

impl DatabaseReference {
    pub fn child(&self, relative: &str) -> DatabaseResult<DatabaseReference> {
        let mut segments = self.path.clone();
        segments.extend(normalize_path(relative)?);
        Ok(DatabaseReference {
            database: self.database.clone(),
            path: segments,
        })
    }

    pub fn set(&self, value: Value) -> DatabaseResult<()> {
        self.database.inner.backend.set(&self.path, value)
    }

    pub fn update(&self, updates: serde_json::Map<String, Value>) -> DatabaseResult<()> {
        for (key, value) in updates {
            self.child(&key)?.set(value)?;
        }
        Ok(())
    }

    pub fn get(&self) -> DatabaseResult<Value> {
        self.database.inner.backend.get(&self.path)
    }

    pub fn path(&self) -> String {
        if self.path.is_empty() {
            "/".to_string()
        } else {
            format!("/{}/", self.path.join("/"))
        }
    }
}

fn normalize_path(path: &str) -> DatabaseResult<Vec<String>> {
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let mut segments = Vec::new();
    for segment in trimmed.split('/') {
        if segment.is_empty() {
            return Err(invalid_argument(
                "Database path cannot contain empty segments",
            ));
        }
        segments.push(segment.to_string());
    }
    Ok(segments)
}

static DATABASE_COMPONENT: LazyLock<()> = LazyLock::new(|| {
    let component = Component::new(
        DATABASE_COMPONENT_NAME,
        Arc::new(database_factory),
        ComponentType::Public,
    )
    .with_instantiation_mode(InstantiationMode::Lazy);
    let _ = app::registry::register_component(component);
});

fn database_factory(
    container: &crate::component::ComponentContainer,
    _options: InstanceFactoryOptions,
) -> Result<DynService, ComponentError> {
    let app = container.root_service::<FirebaseApp>().ok_or_else(|| {
        ComponentError::InitializationFailed {
            name: DATABASE_COMPONENT_NAME.to_string(),
            reason: "Firebase app not attached to component container".to_string(),
        }
    })?;

    let database = Database::new((*app).clone());
    Ok(Arc::new(database) as DynService)
}

fn ensure_registered() {
    LazyLock::force(&DATABASE_COMPONENT);
}

pub fn register_database_component() {
    ensure_registered();
}

pub fn get_database(app: Option<FirebaseApp>) -> DatabaseResult<Arc<Database>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => crate::app::api::get_app(None).map_err(|err| internal_error(err.to_string()))?,
    };

    let provider = app::registry::get_provider(&app, DATABASE_COMPONENT_NAME);
    if let Some(database) = provider.get_immediate::<Database>() {
        return Ok(database);
    }

    match provider.initialize::<Database>(Value::Null, None) {
        Ok(service) => Ok(service),
        Err(crate::component::types::ComponentError::InstanceUnavailable { .. }) => provider
            .get_immediate::<Database>()
            .ok_or_else(|| internal_error("Database component not available")),
        Err(err) => Err(internal_error(err.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::api::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use httpmock::prelude::*;
    use serde_json::json;

    fn unique_settings() -> FirebaseAppSettings {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!(
                "database-{}",
                COUNTER.fetch_add(1, Ordering::SeqCst)
            )),
            ..Default::default()
        }
    }

    #[test]
    fn set_and_get_value() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let ref_root = database.reference("/messages").unwrap();
        ref_root.set(json!({ "greeting": "hello" })).expect("set");
        let value = ref_root.get().unwrap();
        assert_eq!(value, json!({ "greeting": "hello" }));
    }

    #[test]
    fn child_updates_merge() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let root = database.reference("items").unwrap();
        root.set(json!({ "a": { "count": 1 } })).unwrap();
        root.child("a/count").unwrap().set(json!(2)).unwrap();
        let value = root.get().unwrap();
        assert_eq!(value, json!({ "a": { "count": 2 } }));
    }

    #[test]
    fn rest_backend_performs_http_requests() {
        let server = MockServer::start();

        let set_mock = server.mock(|when, then| {
            when.method(PUT)
                .path("/messages.json")
                .json_body(json!({ "greeting": "hello" }));
            then.status(200)
                .header("content-type", "application/json")
                .body("null");
        });

        let get_mock = server.mock(|when, then| {
            when.method(GET).path("/messages.json");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"greeting":"hello"}"#);
        });

        let options = FirebaseOptions {
            project_id: Some("project".into()),
            database_url: Some(server.url("/")),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let reference = database.reference("/messages").unwrap();

        reference
            .set(json!({ "greeting": "hello" }))
            .expect("set over REST");
        let value = reference.get().expect("get over REST");

        assert_eq!(value, json!({ "greeting": "hello" }));
        set_mock.assert();
        get_mock.assert();
    }
}
