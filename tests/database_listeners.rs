#![cfg(not(target_arch = "wasm32"))]

use firebase_rs_sdk::app::initialize_app;
use firebase_rs_sdk::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::database::{get_database, DatabaseResult};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

fn unique_settings(name: &str) -> FirebaseAppSettings {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    FirebaseAppSettings {
        name: Some(format!("{name}-{}", COUNTER.fetch_add(1, Ordering::SeqCst))),
        ..Default::default()
    }
}

async fn init_database(suffix: &str) -> DatabaseResult<Arc<firebase_rs_sdk::database::Database>> {
    let options = FirebaseOptions {
        project_id: Some(format!("listener-tests-{suffix}")),
        ..Default::default()
    };
    let app = initialize_app(options, Some(unique_settings("database-listener")))
        .await
        .expect("initialize app");
    get_database(Some(app)).await
}

#[tokio::test(flavor = "multi_thread")]
async fn value_listener_emits_initial_and_updates() {
    let database = init_database("value").await.unwrap();
    let reference = database.reference("counters/main").unwrap();

    let events: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let captured = events.clone();

    let registration = reference
        .on_value(move |result| {
            if let Ok(snapshot) = result {
                captured.lock().unwrap().push(snapshot.value().clone());
            }
        })
        .await
        .expect("register on_value listener");

    reference.set(json!(1)).await.unwrap();
    reference.set(json!(2)).await.unwrap();

    {
        let events = events.lock().unwrap();
        assert_eq!(events.as_slice(), &[Value::Null, json!(1), json!(2)]);
    }

    registration.detach();
}

#[tokio::test(flavor = "multi_thread")]
async fn child_added_listener_reports_new_children() {
    let database = init_database("child").await.unwrap();
    let list = database.reference("lists/default").unwrap();

    let events: Arc<Mutex<Vec<(Value, Option<String>)>>> = Arc::new(Mutex::new(Vec::new()));
    let captured = events.clone();

    let registration = list
        .on_child_added(move |result| {
            if let Ok(event) = result {
                captured
                    .lock()
                    .unwrap()
                    .push((event.snapshot.into_value(), event.previous_name));
            }
        })
        .await
        .expect("register child_added listener");

    list.child("first").unwrap().set(json!("alpha")).await.unwrap();
    list.child("second").unwrap().set(json!("beta")).await.unwrap();

    {
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], (json!("alpha"), None));
        assert_eq!(events[1], (json!("beta"), Some("first".to_string())));
    }

    registration.detach();
}
