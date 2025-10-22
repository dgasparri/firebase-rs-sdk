//! Demonstrates child event listeners and snapshot utilities in the Realtime Database port.
//!
//! Mirrors the patterns from `packages/database/src/api/Reference_impl.ts` by wiring
//! `on_child_added`, `on_child_changed`, and snapshot traversal helpers. The example
//! assumes an emulator is available but will work against any Realtime Database instance.

use firebase_rs_sdk::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::database::{
    get_database, on_child_added, on_child_changed, on_child_removed, server_timestamp,
};
use serde_json::json;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project_id = prompt("Firebase project ID", "child-events-demo");
    let db_url = prompt(
        "Realtime Database URL",
        "http://127.0.0.1:9000/?ns=child-events-demo",
    );

    let options = FirebaseOptions {
        project_id: Some(project_id.clone()),
        database_url: Some(db_url.clone()),
        ..Default::default()
    };

    let app = initialize_app(options, Some(FirebaseAppSettings::default()))?;
    let database = get_database(Some(app))?;

    let tasks = database.reference("tasks")?;

    let added = Arc::new(Mutex::new(Vec::new()));
    let changed = Arc::new(Mutex::new(Vec::new()));
    let removed = Arc::new(Mutex::new(Vec::new()));

    let added_capture = added.clone();
    let added_registration = on_child_added(&tasks, move |snapshot, prev| {
        added_capture
            .lock()
            .unwrap()
            .push((snapshot.key().unwrap_or("<root>").to_string(), prev.clone()));
    })?;

    let changed_capture = changed.clone();
    let changed_registration = on_child_changed(&tasks, move |snapshot, _prev| {
        changed_capture.lock().unwrap().push((
            snapshot.key().unwrap_or("<root>").to_string(),
            snapshot.to_json(),
        ));
    })?;

    let removed_capture = removed.clone();
    let removed_registration = on_child_removed(&tasks, move |snapshot, _prev| {
        removed_capture
            .lock()
            .unwrap()
            .push(snapshot.key().unwrap_or("<root>").to_string());
    })?;

    // Drive some changes.
    let alpha = tasks.child("alpha")?;
    alpha.set(json!({ "title": "Create project", "created_at": server_timestamp() }))?;

    let beta = tasks.child("beta")?;
    beta.set(json!({ "title": "Review PR", "created_at": server_timestamp() }))?;
    beta.child("title")?.set(json!("Review PR comments"))?;

    alpha.remove()?;

    println!("child_added events: {:?}", added.lock().unwrap());
    println!("child_changed events: {:?}", changed.lock().unwrap());
    println!("child_removed events: {:?}", removed.lock().unwrap());

    added_registration.detach();
    changed_registration.detach();
    removed_registration.detach();

    Ok(())
}

fn prompt(label: &str, default: &str) -> String {
    print!("{label} [{default}]: ");
    io::stdout().flush().expect("flush prompt");
    let mut buffer = String::new();
    io::stdin().read_line(&mut buffer).expect("read input");
    let trimmed = buffer.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}
