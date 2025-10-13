use std::sync::{Arc, LazyLock};

use crate::app;
use crate::app::api::{get_app, register_version};
use crate::app::FirebaseApp;
use crate::app::SDK_VERSION;
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType};
use crate::firestore::constants::FIRESTORE_COMPONENT_NAME;
use crate::firestore::error::{
    internal_error, invalid_argument, missing_project_id, FirestoreResult,
};
use crate::firestore::model::{DatabaseId, ResourcePath};

use super::reference::{CollectionReference, DocumentReference};

#[derive(Clone, Debug)]
pub struct Firestore {
    inner: Arc<FirestoreInner>,
}

#[derive(Debug)]
struct FirestoreInner {
    app: FirebaseApp,
    database_id: DatabaseId,
}

impl Firestore {
    pub(crate) fn new(app: FirebaseApp, database_id: DatabaseId) -> Self {
        let inner = FirestoreInner { app, database_id };
        Self {
            inner: Arc::new(inner),
        }
    }

    /// Returns the `FirebaseApp` this Firestore instance is scoped to.
    pub fn app(&self) -> &FirebaseApp {
        &self.inner.app
    }

    /// The fully qualified database identifier (project + database name).
    pub fn database_id(&self) -> &DatabaseId {
        &self.inner.database_id
    }

    /// Creates a `CollectionReference` pointing at `path`.
    ///
    /// The path is interpreted relative to the Firestore root using forward
    /// slashes to separate segments (e.g. `"users/alovelace/repos"`).
    pub fn collection(&self, path: &str) -> FirestoreResult<CollectionReference> {
        let resource = ResourcePath::from_string(path)?;
        CollectionReference::new(self.clone(), resource)
    }

    /// Creates a `DocumentReference` pointing at `path`.
    ///
    /// The path must contain an even number of segments (collection/doc pairs).
    pub fn doc(&self, path: &str) -> FirestoreResult<DocumentReference> {
        let resource = ResourcePath::from_string(path)?;
        DocumentReference::new(self.clone(), resource)
    }

    /// Clones a Firestore handle that has been wrapped in an `Arc`.
    pub fn from_arc(arc: Arc<Self>) -> Self {
        arc.as_ref().clone()
    }

    /// Returns the project identifier backing this database.
    pub fn project_id(&self) -> &str {
        self.inner.database_id.project_id()
    }

    /// Returns the logical database name (usually `"(default)"`).
    pub fn database(&self) -> &str {
        self.inner.database_id.database()
    }
}

static FIRESTORE_COMPONENT: LazyLock<()> = LazyLock::new(|| {
    let component = Component::new(
        FIRESTORE_COMPONENT_NAME,
        Arc::new(firestore_factory),
        ComponentType::Public,
    )
    .with_instantiation_mode(InstantiationMode::Lazy)
    .with_multiple_instances(true);

    let _ = app::registry::register_component(component);
});

fn firestore_factory(
    container: &crate::component::ComponentContainer,
    options: InstanceFactoryOptions,
) -> Result<DynService, ComponentError> {
    let app = container.root_service::<FirebaseApp>().ok_or_else(|| {
        ComponentError::InitializationFailed {
            name: FIRESTORE_COMPONENT_NAME.to_string(),
            reason: "Firebase app not attached to component container".to_string(),
        }
    })?;

    let database_id = match options.instance_identifier.as_deref() {
        Some(identifier) if !identifier.is_empty() => parse_database_identifier(&app, identifier)
            .map_err(|err| {
            ComponentError::InitializationFailed {
                name: FIRESTORE_COMPONENT_NAME.to_string(),
                reason: err.to_string(),
            }
        })?,
        _ => DatabaseId::from_app(&app).map_err(|err| ComponentError::InitializationFailed {
            name: FIRESTORE_COMPONENT_NAME.to_string(),
            reason: err.to_string(),
        })?,
    };

    let firestore = Firestore::new((*app).clone(), database_id);

    register_version("@firebase/firestore", SDK_VERSION, None);

    Ok(Arc::new(firestore) as DynService)
}

fn parse_database_identifier(app: &FirebaseApp, identifier: &str) -> FirestoreResult<DatabaseId> {
    let options = app.options();
    let project_id = options.project_id.clone().ok_or_else(missing_project_id)?;

    if identifier.starts_with("projects/") {
        let segments: Vec<_> = identifier.split('/').collect();
        if segments.len() == 4 && segments[0] == "projects" && segments[2] == "databases" {
            return Ok(DatabaseId::new(segments[1], segments[3]));
        }
        return Err(invalid_argument(
            "Database identifier must follow projects/{project}/databases/{database}",
        ));
    }

    Ok(DatabaseId::new(project_id, identifier))
}

fn ensure_registered() {
    LazyLock::force(&FIRESTORE_COMPONENT);
}

pub fn register_firestore_component() {
    ensure_registered();
}

/// Resolves (or lazily instantiates) the Firestore service for the provided app.
///
/// When `app` is `None` the default Firebase app is used. Multiple calls with
/// the same app yield the same shared `Arc<Firestore>` handle.
pub fn get_firestore(app: Option<FirebaseApp>) -> FirestoreResult<Arc<Firestore>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => get_app(None).map_err(|err| internal_error(err.to_string()))?,
    };

    let provider = app::registry::get_provider(&app, FIRESTORE_COMPONENT_NAME);
    provider
        .get_immediate_with_options::<Firestore>(None, false)
        .map_err(|err| internal_error(err.to_string()))?
        .ok_or_else(|| internal_error("Failed to obtain Firestore instance"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::api::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};

    fn unique_settings() -> FirebaseAppSettings {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!(
                "firestore-api-{}",
                COUNTER.fetch_add(1, Ordering::SeqCst)
            )),
            ..Default::default()
        }
    }

    #[test]
    fn get_firestore_registers_component() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let firestore = get_firestore(Some(app)).unwrap();
        assert_eq!(firestore.project_id(), "project");
        assert_eq!(firestore.database(), "(default)");
    }

    #[test]
    fn custom_database_identifier() {
        register_firestore_component();
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let provider = app::registry::get_provider(&app, FIRESTORE_COMPONENT_NAME);
        let instance = provider
            .initialize::<Firestore>(
                serde_json::Value::Null,
                Some("projects/project/databases/custom"),
            )
            .unwrap();
        assert_eq!(instance.database(), "custom");
    }
}
