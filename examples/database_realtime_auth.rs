use std::error::Error;
use std::io::{self, Write};
use std::sync::Arc;
use std::time::Duration;

use firebase_rs_sdk_unofficial::app::api::{initialize_app, SDK_VERSION};
use firebase_rs_sdk_unofficial::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk_unofficial::auth::api::{auth_for_app, register_auth_component};
use firebase_rs_sdk_unofficial::auth::Auth;
use firebase_rs_sdk_unofficial::database::*;
use serde_json::json;

fn prompt(prompt: &str) -> io::Result<String> {
    print!("{prompt}: ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

fn ensure_logged_in(auth: &Arc<Auth>, email: &str, password: &str) -> Result<(), Box<dyn Error>> {
    match auth.current_user() {
        Some(user) if user.email_verified() => {
            println!(
                "Already signed in as {}",
                user.info().email.clone().unwrap_or("<unknown>".to_string())
            );
            Ok(())
        }
        Some(_) | None => {
            auth.sign_in_with_email_and_password(email, password)?;
            println!("Signed in as {email}");
            Ok(())
        }
    }
}

fn attach_listener(reference: &DatabaseReference) -> DatabaseResult<ListenerRegistration> {
    reference.on_value(|snapshot: DataSnapshot| {
        if snapshot.exists() {
            println!(
                "Current data at {}: {}",
                snapshot.reference().path(),
                snapshot.value()
            );
        } else {
            println!("{} is empty", snapshot.reference().path());
        }
    })
}

fn attach_query_listener(query: &DatabaseQuery) -> DatabaseResult<ListenerRegistration> {
    query.on_value(|snapshot| {
        println!("Latest scores snapshot: {}", snapshot.value());
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("Firebase Rust SDK demo ({SDK_VERSION})\n");

    let api_key = prompt("Enter your Firebase Web API key")?;
    let project_id = prompt("Enter your Firebase project ID")?;
    let database_url =
        prompt("Enter the Realtime Database URL (e.g. https://<project>.firebaseio.com)")?;
    let email = prompt("Email")?;
    let password = prompt("Password")?;
    let bucket = prompt("Database bucket/path to write (e.g. demo/chat)")?;

    let options = FirebaseOptions {
        api_key: Some(api_key.clone()),
        project_id: Some(project_id.clone()),
        database_url: Some(database_url.clone()),
        auth_domain: Some(format!("{project_id}.firebaseapp.com")),
        ..Default::default()
    };

    let app = initialize_app(options, Some(FirebaseAppSettings::default()))?;

    register_auth_component();
    let auth = auth_for_app(app.clone())?;
    ensure_logged_in(&auth, &email, &password)?;

    let database = get_database(Some(app.clone()))?;
    let bucket_reference = database.reference(&bucket)?;

    let _bucket_listener = attach_listener(&bucket_reference)?;

    let highscores_query = query(
        bucket_reference.child("scores")?,
        vec![order_by_child("score"), limit_to_last(5)],
    )?;
    let _scores_listener = attach_query_listener(&highscores_query)?;

    println!("\nWriting sample data...");
    bucket_reference
        .child("messages")?
        .child("welcome")?
        .set(json!({ "from": email, "text": "Hello from Rust!" }))?;

    let score_ref = push_child(&bucket_reference.child("scores")?);
    score_ref.set(json!({ "user": email, "score": 42 }))?;

    println!("Data written. Press Enter to exit.");
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;

    println!("Cleaning up... deleting sample score entry");
    std::thread::sleep(Duration::from_millis(200));
    score_ref.remove()?;

    Ok(())
}

fn push_child(reference: &DatabaseReference) -> DatabaseReference {
    use rand::{distributions::Alphanumeric, thread_rng, Rng};

    let mut rng = thread_rng();
    loop {
        let candidate: String = (0..20).map(|_| rng.sample(Alphanumeric) as char).collect();
        if !candidate.starts_with('-') {
            break reference.child(&candidate).expect("push child");
        }
    }
}
