use std::sync::{Arc, LazyLock, Mutex};

use serde_json::Value;

use crate::app;
use crate::app::FirebaseApp;
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType};
use crate::database::constants::DATABASE_COMPONENT_NAME;
use crate::database::error::{internal_error, invalid_argument, DatabaseResult};

#[derive(Clone, Debug)]
pub struct Database {
    inner: Arc<DatabaseInner>,
}

#[derive(Debug)]
struct DatabaseInner {
    app: FirebaseApp,
    data: Mutex<Value>,
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
                app,
                data: Mutex::new(Value::Object(Default::default())),
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
        let mut data = self.database.inner.data.lock().unwrap();
        set_at_path(&mut *data, &self.path, value);
        Ok(())
    }

    pub fn update(&self, updates: serde_json::Map<String, Value>) -> DatabaseResult<()> {
        for (key, value) in updates {
            self.child(&key)?.set(value)?;
        }
        Ok(())
    }

    pub fn get(&self) -> DatabaseResult<Value> {
        let data = self.database.inner.data.lock().unwrap();
        Ok(get_at_path(&*data, &self.path)
            .cloned()
            .unwrap_or(Value::Null))
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
}
