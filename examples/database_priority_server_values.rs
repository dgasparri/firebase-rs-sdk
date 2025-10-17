//! Demonstrates Realtime Database priority writes and server value helpers.
//!
//! Mirrors the JS SDK patterns from `setWithPriority`, `setPriority`, and
//! `ServerValue` helpers (`packages/database/src/api/Reference_impl.ts` and
//! `packages/database/src/api/ServerValue.ts`).

use firebase_rs_sdk_unofficial::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk_unofficial::database::error::DatabaseResult;
use firebase_rs_sdk_unofficial::database::{
    get_database, increment, push_with_value, server_timestamp, set_priority, set_with_priority,
};
use serde_json::json;
use std::io::{self, Write};

fn main() -> DatabaseResult<()> {
    // Prompt the operator for runtime configuration so the example can target
    // either a local emulator or a hosted database instance.
    let project_id = prompt_with_default("Firebase project ID", "priority-demo");
    let database_url = prompt_with_default(
        "Realtime Database URL (emulator or prod endpoint)",
        "http://127.0.0.1:9000/?ns=priority-demo",
    );
    let task_title = prompt_with_default("Task title", "Refill coffee beans");
    let priority_value = prompt_with_default("Initial priority (number)", "10")
        .parse::<f64>()
        .unwrap_or_else(|_| {
            eprintln!("Using default priority 10 (failed to parse input)");
            10.0
        });
    let priority_lower = prompt_with_default("Follow-up priority (number)", "5")
        .parse::<f64>()
        .unwrap_or_else(|_| {
            eprintln!("Using fallback priority 5 (failed to parse input)");
            5.0
        });
    let increment_delta = prompt_with_default("Increment delta", "1.0")
        .parse::<f64>()
        .unwrap_or_else(|_| {
            eprintln!("Using fallback increment 1.0 (failed to parse input)");
            1.0
        });

    let options = FirebaseOptions {
        project_id: Some(project_id.clone()),
        database_url: Some(database_url.clone()),
        ..Default::default()
    };

    let app = initialize_app(options, Some(FirebaseAppSettings::default()))
        .expect("initialize app with database settings");
    let database = get_database(Some(app))?;

    // Create a collection of tasks and push one with explicit priority.
    let tasks = database.reference("tasks")?;
    let task = push_with_value(
        &tasks,
        json!({
            "title": task_title,
            "created_at": server_timestamp(),
        }),
    )?;
    set_with_priority(&task, json!({ "done": false }), priority_value)?;

    // Later, adjust the priority and bump a counter atomically.
    set_priority(&task, priority_lower)?;
    let stats = database
        .reference("stats/processed")
        .expect("stats reference");
    stats.set(json!(0))?;
    stats.set(increment(increment_delta))?;

    println!(
        "Created task {} under project '{}' using database '{}'",
        task.key().unwrap_or("<root>"),
        project_id,
        database_url
    );

    Ok(())
}

fn prompt_with_default(prompt: &str, default: &str) -> String {
    print!("{prompt} [{default}]: ");
    io::stdout().flush().expect("flush prompt");
    let mut buffer = String::new();
    io::stdin().read_line(&mut buffer).expect("read user input");
    let trimmed = buffer.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}
