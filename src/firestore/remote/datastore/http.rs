use std::collections::BTreeMap;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use reqwest::Method;

use crate::firestore::api::query::QueryDefinition;
use crate::firestore::api::{DocumentSnapshot, SnapshotMetadata};
use crate::firestore::error::{
    internal_error, invalid_argument, FirestoreError, FirestoreErrorCode, FirestoreResult,
};
use crate::firestore::model::{DatabaseId, DocumentKey};
use crate::firestore::remote::connection::{Connection, ConnectionBuilder, RequestContext};
use crate::firestore::remote::serializer::JsonProtoSerializer;
use crate::firestore::value::MapValue;
use serde_json::json;

use super::{Datastore, NoopTokenProvider, TokenProviderArc};

#[derive(Clone)]
pub struct HttpDatastore {
    connection: Connection,
    serializer: JsonProtoSerializer,
    auth_provider: TokenProviderArc,
    app_check_provider: TokenProviderArc,
    retry: RetrySettings,
}

#[derive(Clone)]
pub struct HttpDatastoreBuilder {
    database_id: DatabaseId,
    connection_builder: ConnectionBuilder,
    auth_provider: TokenProviderArc,
    app_check_provider: TokenProviderArc,
    retry: RetrySettings,
}

#[derive(Clone, Debug)]
pub struct RetrySettings {
    pub max_attempts: usize,
    pub initial_delay: Duration,
    pub multiplier: f64,
    pub max_delay: Duration,
    pub request_timeout: Duration,
}

impl Default for RetrySettings {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            initial_delay: Duration::from_millis(100),
            multiplier: 1.5,
            max_delay: Duration::from_secs(5),
            request_timeout: Duration::from_secs(20),
        }
    }
}

impl HttpDatastore {
    pub fn builder(database_id: DatabaseId) -> HttpDatastoreBuilder {
        HttpDatastoreBuilder::new(database_id)
    }

    pub fn from_database_id(database_id: DatabaseId) -> FirestoreResult<Self> {
        Self::builder(database_id).build()
    }

    fn new(
        connection: Connection,
        serializer: JsonProtoSerializer,
        auth_provider: TokenProviderArc,
        app_check_provider: TokenProviderArc,
        retry: RetrySettings,
    ) -> Self {
        Self {
            connection,
            serializer,
            auth_provider,
            app_check_provider,
            retry,
        }
    }

    fn execute_with_retry<F, T>(&self, mut operation: F) -> FirestoreResult<T>
    where
        F: FnMut(&RequestContext) -> FirestoreResult<T>,
    {
        let mut attempt = 0usize;
        loop {
            let context = self.build_request_context()?;
            match operation(&context) {
                Ok(result) => return Ok(result),
                Err(err) => {
                    if !self.retry.should_retry(attempt, &err) {
                        return Err(err);
                    }

                    if err.code == FirestoreErrorCode::Unauthenticated {
                        self.auth_provider.invalidate_token();
                        self.app_check_provider.invalidate_token();
                    }

                    let delay = self.retry.backoff_delay(attempt);
                    thread::sleep(delay);
                    attempt += 1;
                }
            }
        }
    }

    fn build_request_context(&self) -> FirestoreResult<RequestContext> {
        let auth_token = self.auth_provider.get_token()?;
        let app_check_token = self.app_check_provider.get_token()?;
        Ok(RequestContext {
            auth_token,
            app_check_token,
            request_timeout: Some(self.retry.request_timeout),
        })
    }
}

impl Datastore for HttpDatastore {
    fn get_document(&self, key: &DocumentKey) -> FirestoreResult<DocumentSnapshot> {
        let doc_path = format!("documents/{}", key.path().canonical_string());
        let snapshot = self.execute_with_retry(|context| {
            self.connection
                .invoke_json_optional(Method::GET, &doc_path, None, context)
        })?;

        if let Some(json) = snapshot {
            let map_value = self
                .serializer
                .decode_document_fields(&json)?
                .unwrap_or_else(|| MapValue::new(std::collections::BTreeMap::new()));
            Ok(DocumentSnapshot::new(
                key.clone(),
                Some(map_value),
                SnapshotMetadata::new(false, false),
            ))
        } else {
            Ok(DocumentSnapshot::new(
                key.clone(),
                None,
                SnapshotMetadata::new(false, false),
            ))
        }
    }

    fn set_document(&self, key: &DocumentKey, data: MapValue, merge: bool) -> FirestoreResult<()> {
        if merge {
            return Err(invalid_argument(
                "HTTP datastore set with merge is not yet implemented",
            ));
        }

        let commit_body = self.serializer.encode_commit_body(key, &data);
        self.execute_with_retry(|context| {
            self.connection
                .invoke_json(
                    Method::POST,
                    "documents:commit",
                    Some(commit_body.clone()),
                    context,
                )
                .map(|_| ())
        })
    }

    fn run_query(&self, query: &QueryDefinition) -> FirestoreResult<Vec<DocumentSnapshot>> {
        let request_path = if query.parent_path().is_empty() {
            "documents:runQuery".to_string()
        } else {
            format!(
                "documents/{}:runQuery",
                query.parent_path().canonical_string()
            )
        };

        let body = json!({
            "structuredQuery": {
                "from": [ {
                    "collectionId": query.collection_id(),
                    "allDescendants": false
                } ]
            }
        });

        let response = self.execute_with_retry(|context| {
            self.connection
                .invoke_json(Method::POST, &request_path, Some(body.clone()), context)
        })?;

        let results = response
            .as_array()
            .ok_or_else(|| internal_error("Firestore runQuery response must be an array"))?;

        let mut snapshots = Vec::new();
        for entry in results {
            let document = match entry.get("document") {
                Some(value) => value,
                None => continue,
            };

            let name = document
                .get("name")
                .and_then(|value| value.as_str())
                .ok_or_else(|| {
                    internal_error("Firestore runQuery document missing 'name' field")
                })?;
            let key = self.parse_document_name(name)?;

            let map_value = self
                .serializer
                .decode_document_fields(document)?
                .unwrap_or_else(|| MapValue::new(BTreeMap::new()));

            snapshots.push(DocumentSnapshot::new(
                key,
                Some(map_value),
                SnapshotMetadata::new(false, false),
            ));
        }

        Ok(snapshots)
    }
}

impl HttpDatastore {
    fn parse_document_name(&self, name: &str) -> FirestoreResult<DocumentKey> {
        let prefix = format!("{}/documents/", self.serializer.database_name());
        if !name.starts_with(&prefix) {
            return Err(internal_error(format!(
                "Unexpected document name '{name}' returned by Firestore"
            )));
        }

        let relative = &name[prefix.len()..];
        DocumentKey::from_string(relative)
    }
}

impl HttpDatastoreBuilder {
    fn new(database_id: DatabaseId) -> Self {
        let auth_provider: TokenProviderArc = Arc::new(NoopTokenProvider::default());
        let app_check_provider: TokenProviderArc = Arc::new(NoopTokenProvider::default());
        let connection_builder = Connection::builder(database_id.clone());
        Self {
            database_id,
            connection_builder,
            auth_provider,
            app_check_provider,
            retry: RetrySettings::default(),
        }
    }

    pub fn with_auth_provider(mut self, provider: TokenProviderArc) -> Self {
        self.auth_provider = provider;
        self
    }

    pub fn with_app_check_provider(mut self, provider: TokenProviderArc) -> Self {
        self.app_check_provider = provider;
        self
    }

    pub fn with_retry_settings(mut self, settings: RetrySettings) -> Self {
        self.retry = settings;
        self
    }

    pub fn with_connection_builder(mut self, builder: ConnectionBuilder) -> Self {
        self.connection_builder = builder;
        self
    }

    pub fn build(self) -> FirestoreResult<HttpDatastore> {
        let connection = self.connection_builder.build()?;
        let serializer = JsonProtoSerializer::new(self.database_id.clone());
        Ok(HttpDatastore::new(
            connection,
            serializer,
            self.auth_provider,
            self.app_check_provider,
            self.retry,
        ))
    }
}

impl RetrySettings {
    fn should_retry(&self, attempt: usize, error: &FirestoreError) -> bool {
        if attempt + 1 >= self.max_attempts {
            return false;
        }

        matches!(
            error.code,
            FirestoreErrorCode::Internal
                | FirestoreErrorCode::Unavailable
                | FirestoreErrorCode::DeadlineExceeded
                | FirestoreErrorCode::ResourceExhausted
                | FirestoreErrorCode::Unauthenticated
        )
    }

    fn backoff_delay(&self, attempt: usize) -> Duration {
        let factor = self.multiplier.powi(attempt as i32);
        let delay = self.initial_delay.mul_f64(factor);
        if delay > self.max_delay {
            self.max_delay
        } else {
            delay
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firestore::api::query::QueryDefinition;
    use crate::firestore::error::{internal_error, unauthenticated};
    use crate::firestore::model::{DatabaseId, ResourcePath};
    use crate::test_support::start_mock_server;
    use httpmock::prelude::*;
    use serde_json::json;
    use std::panic;

    #[test]
    fn retries_unauthenticated_errors() {
        let settings = RetrySettings {
            max_attempts: 3,
            ..Default::default()
        };
        let error = unauthenticated("expired");
        assert!(settings.should_retry(0, &error));
        assert!(settings.should_retry(1, &error));
        assert!(!settings.should_retry(2, &error));
    }

    #[test]
    fn stops_retrying_after_max_attempts() {
        let settings = RetrySettings {
            max_attempts: 1,
            ..Default::default()
        };
        let error = internal_error("boom");
        assert!(!settings.should_retry(0, &error));
    }

    #[test]
    fn run_query_fetches_documents() {
        let server = match panic::catch_unwind(|| start_mock_server()) {
            Ok(server) => server,
            Err(_) => {
                eprintln!(
                    "Skipping run_query_fetches_documents: unable to bind httpmock server in this environment."
                );
                return;
            }
        };
        let database_id = DatabaseId::new("demo-project", "(default)");

        let response_body = json!([
            {
                "document": {
                    "name": format!(
                        "projects/{}/databases/{}/documents/cities/LA",
                        database_id.project_id(),
                        database_id.database()
                    ),
                    "fields": {
                        "name": { "stringValue": "Los Angeles" }
                    }
                }
            },
            {
                "document": {
                    "name": format!(
                        "projects/{}/databases/{}/documents/cities/SF",
                        database_id.project_id(),
                        database_id.database()
                    ),
                    "fields": {
                        "name": { "stringValue": "San Francisco" }
                    }
                }
            }
        ]);

        let expected_body = json!({
            "structuredQuery": {
                "from": [
                    {
                        "collectionId": "cities",
                        "allDescendants": false
                    }
                ]
            }
        });

        let expected_path = format!(
            "/v1/projects/{}/databases/{}/documents:runQuery",
            database_id.project_id(),
            database_id.database()
        );

        let run_query_path = expected_path.clone();
        let expected_body_clone = expected_body.clone();
        let response_clone = response_body.clone();

        let _mock = server.mock(move |when, then| {
            when.method(POST)
                .path(run_query_path.as_str())
                .json_body(expected_body_clone.clone());
            then.status(200).json_body(response_clone.clone());
        });

        let client = reqwest::blocking::Client::builder()
            .build()
            .expect("reqwest client");

        let connection_builder = Connection::builder(database_id.clone())
            .with_client(client)
            .with_emulator_host(server.address().to_string());

        let datastore = HttpDatastore::builder(database_id.clone())
            .with_connection_builder(connection_builder)
            .build()
            .expect("datastore");

        let definition = QueryDefinition {
            collection_path: ResourcePath::from_string("cities").unwrap(),
            parent_path: ResourcePath::root(),
            collection_id: "cities".to_string(),
        };

        let snapshots = datastore.run_query(&definition).expect("query");
        assert_eq!(snapshots.len(), 2);
        let names: Vec<_> = snapshots.iter().map(|snap| snap.id().to_string()).collect();
        assert_eq!(names, vec!["LA", "SF"]);
    }
}
