//! Helpers used by documentation examples to compile in isolation.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::app::{initialize_app, FirebaseApp, FirebaseAppSettings, FirebaseOptions};
use crate::auth::{auth_for_app, register_auth_component, Auth};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

pub async fn get_mock_app() -> FirebaseApp {
    let app_name = format!("doc-auth-{}", COUNTER.fetch_add(1, Ordering::SeqCst));
    let options = FirebaseOptions {
        api_key: Some("DOCTEST_API_KEY".into()),
        project_id: Some("doctest-project".into()),
        auth_domain: Some("doctest.firebaseapp.com".into()),
        ..Default::default()
    };
    let settings = FirebaseAppSettings {
        name: Some(app_name),
        ..Default::default()
    };

    initialize_app(options, Some(settings))
        .await
        .expect("failed to initialize doctest Firebase app")
}

pub async fn get_mock_auth(app: Option<FirebaseApp>) -> Arc<Auth> {
    let app = match app {
        Some(a) => a,
        None => get_mock_app().await,
    };
    register_auth_component();
    auth_for_app(app).expect("failed to resolve Auth component")
}

pub mod auth {

    use crate::auth::{ApplicationVerifier, AuthResult};

    pub struct MockVerifier {
        pub token: &'static str,
        pub kind: &'static str,
    }

    impl ApplicationVerifier for MockVerifier {
        fn verify(&self) -> AuthResult<String> {
            Ok(self.token.to_string())
        }

        fn verifier_type(&self) -> &str {
            self.kind
        }
    }
}

pub mod firestore {
    use crate::firestore::{
        get_firestore, register_firestore_component, Firestore, FirestoreClient,
    };
    use std::sync::Arc;

    pub async fn get_mock_firestore(app: Option<super::FirebaseApp>) -> Arc<Firestore> {
        let app = match app {
            Some(a) => a,
            None => super::get_mock_app().await,
        };

        register_firestore_component();
        get_firestore(Some(app))
            .await
            .expect("failed to resolve Firestore component")
    }

    pub async fn get_mock_client(app: Option<super::FirebaseApp>) -> FirestoreClient {
        let firestore = get_mock_firestore(app).await;
        FirestoreClient::with_http_datastore(Firestore::from_arc(firestore))
            .expect("failed to create FirestoreClient")
    }
}
