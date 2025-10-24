use firebase_rs_sdk::app::*;
use firebase_rs_sdk::app_check::*;
use firebase_rs_sdk::auth::*;
use firebase_rs_sdk::firestore::*;
use std::collections::BTreeMap;
use std::time::Duration;

#[allow(dead_code)]
async fn insert_documents() -> Result<(), Box<dyn std::error::Error>> {
    // TODO: replace with your project configuration
    let options = FirebaseOptions {
        project_id: Some("your-project".into()),
        // add other Firebase options as needed
        ..Default::default()
    };
    let app = initialize_app(options, Some(FirebaseAppSettings::default())).await?;
    let auth = auth_for_app(app.clone())?;
    // Optional: wire App Check tokens into Firestore.
    let app_check_provider =
        custom_provider(|| token_with_ttl("fake-token", Duration::from_secs(60)));
    let app_check =
        initialize_app_check(Some(app.clone()), AppCheckOptions::new(app_check_provider)).await?;
    let app_check_internal = FirebaseAppCheckInternal::new(app_check);
    let firestore = get_firestore(Some(app.clone())).await?;
    let client = FirestoreClient::with_http_datastore_authenticated(
        Firestore::from_arc(firestore.clone()),
        auth.token_provider(),
        Some(app_check_internal.token_provider()),
    )?;
    let mut ada = BTreeMap::new();
    ada.insert("first".into(), FirestoreValue::from_string("Ada"));
    ada.insert("last".into(), FirestoreValue::from_string("Lovelace"));
    ada.insert("born".into(), FirestoreValue::from_integer(1815));
    let ada_snapshot = client.add_doc("users", ada).await?;
    println!("Document written with ID: {}", ada_snapshot.id());
    let mut alan = BTreeMap::new();
    alan.insert("first".into(), FirestoreValue::from_string("Alan"));
    alan.insert("middle".into(), FirestoreValue::from_string("Mathison"));
    alan.insert("last".into(), FirestoreValue::from_string("Turing"));
    alan.insert("born".into(), FirestoreValue::from_integer(1912));
    let alan_snapshot = client.add_doc("users", alan).await?;
    println!("Document written with ID: {}", alan_snapshot.id());
    Ok(())
}

fn main() {
    eprintln!("Example of Firestore document insertion. See source code for details.");
}
