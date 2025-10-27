use firebase_rs_sdk::app::*;
use firebase_rs_sdk::firestore::*;
use std::collections::BTreeMap;

#[allow(dead_code)]
async fn insert_documents() -> Result<(), Box<dyn std::error::Error>> {
    // TODO: replace with your project configuration
    let options = FirebaseOptions {
        project_id: Some("your-project".into()),
        // add other Firebase options as needed
        ..Default::default()
    };
    let app = initialize_app(options, Some(FirebaseAppSettings::default())).await?;
    let firestore = get_firestore(Some(app.clone())).await?;
    let client = FirestoreClient::with_http_datastore(Firestore::from_arc(firestore.clone()))?;
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
