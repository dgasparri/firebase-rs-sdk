#![cfg(all(target_arch = "wasm32", feature = "wasm-web"))]

use firebase_rs_sdk::app::api::initialize_app;
use firebase_rs_sdk::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::database::{get_database, DatabaseResult};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

fn unique_settings(name: &str) -> FirebaseAppSettings {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    FirebaseAppSettings {
        name: Some(format!("{name}-{}", COUNTER.fetch_add(1, Ordering::SeqCst))),
        ..Default::default()
    }
}

async fn init_database(label: &str) -> DatabaseResult<Arc<firebase_rs_sdk::database::Database>> {
    let options = FirebaseOptions {
        project_id: Some(format!("wasm-listener-{label}")),
        ..Default::default()
    };
    let app = initialize_app(options, Some(unique_settings("wasm-database-listener")))
        .await
        .expect("initialize app");
    get_database(Some(app)).await
}

#[wasm_bindgen_test(async)]
async fn wasm_value_listener_receives_updates() {
    let database = init_database("value").await.unwrap();
    let reference = database.reference("wasm/counters/main").unwrap();

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

    reference.set(json!("first")).await.unwrap();
    reference.set(json!("second")).await.unwrap();

    {
        let events = events.lock().unwrap();
        assert_eq!(
            events.as_slice(),
            &[Value::Null, json!("first"), json!("second")]
        );
    }

    registration.detach();
}

#[wasm_bindgen_test(async)]
async fn wasm_child_added_listener_reports_new_entries() {
    let database = init_database("child").await.unwrap();
    let list = database.reference("wasm/lists/default").unwrap();

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

    list.child("alpha").unwrap().set(json!(1)).await.unwrap();
    list.child("beta").unwrap().set(json!(2)).await.unwrap();

    {
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], (json!(1), None));
        assert_eq!(events[1], (json!(2), Some("alpha".to_string())));
    }

    registration.detach();
}
