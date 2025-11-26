use crate::app::FirebaseApp;
use crate::firestore::constants::DEFAULT_DATABASE_ID;
use crate::firestore::error::{missing_project_id, FirestoreResult};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DatabaseId {
    project_id: String,
    database: String,
}

impl DatabaseId {
    pub fn new(project_id: impl Into<String>, database: impl Into<String>) -> Self {
        Self {
            project_id: project_id.into(),
            database: database.into(),
        }
    }

    pub fn default(project_id: impl Into<String>) -> Self {
        Self::new(project_id, DEFAULT_DATABASE_ID)
    }

    pub fn from_app(app: &FirebaseApp) -> FirestoreResult<Self> {
        let options = app.options();
        let project_id = options.project_id.clone().ok_or_else(missing_project_id)?;
        Ok(Self::default(project_id))
    }

    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    pub fn database(&self) -> &str {
        &self.database
    }

    pub fn with_database(&self, database: impl Into<String>) -> Self {
        Self::new(self.project_id.clone(), database)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::initialize_app;
    use crate::app::FirebaseAppSettings;
    use crate::app::FirebaseOptions;

    fn unique_settings() -> FirebaseAppSettings {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!("firestore-db-{}", COUNTER.fetch_add(1, Ordering::SeqCst))),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn builds_from_app() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).await.unwrap();
        let db = DatabaseId::from_app(&app).unwrap();
        assert_eq!(db.project_id(), "project");
        assert_eq!(db.database(), DEFAULT_DATABASE_ID);
    }

    #[tokio::test]
    async fn missing_project_id_errors() {
        let options = FirebaseOptions {
            api_key: Some("test".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).await.unwrap();
        let err = DatabaseId::from_app(&app).unwrap_err();
        assert_eq!(err.code_str(), "firestore/missing-project-id");
    }
}
