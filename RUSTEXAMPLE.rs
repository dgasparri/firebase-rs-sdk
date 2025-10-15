use std::collections::BTreeMap;
use std::error::Error;

use firebase_rs_sdk_unofficial::app::api::initialize_app;
use firebase_rs_sdk_unofficial::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk_unofficial::firestore::api::{get_firestore, Firestore, FirestoreClient};
use firebase_rs_sdk_unofficial::firestore::error::FirestoreResult;
use firebase_rs_sdk_unofficial::firestore::value::{FirestoreValue, ValueKind};

fn main() -> Result<(), Box<dyn Error>> {
    // TODO: Replace these placeholder options with the values from your Firebase project.
    let firebase_config = FirebaseOptions {
        api_key: Some("demo-api-key".into()),
        project_id: Some("demo-project".into()),
        ..Default::default()
    };

    let app = initialize_app(firebase_config, Some(FirebaseAppSettings::default()))?;
    let firestore_arc = get_firestore(Some(app.clone()))?;
    let firestore = Firestore::from_arc(firestore_arc);

    // Talk to the hosted Firestore REST API. Configure credentials/tokens as needed.
    let client = FirestoreClient::with_http_datastore(firestore.clone())?;

    let cities = load_cities(&firestore, &client)?;

    println!("Loaded {} cities from Firestore:", cities.len());
    for city in cities {
        let name = field_as_string(&city, "name").unwrap_or_else(|| "Unknown".into());
        let state = field_as_string(&city, "state").unwrap_or_else(|| "Unknown".into());
        let country = field_as_string(&city, "country").unwrap_or_else(|| "Unknown".into());
        let population = field_as_i64(&city, "population").unwrap_or_default();
        println!("- {name}, {state} ({country}) — population {population}");
    }

    Ok(())
}

/// Mirrors the `getCities` helper in `JSEXAMPLE.ts`, but targets real documents stored in
/// Firestore. Populate the referenced documents in your Firestore instance before running
/// the code, or adapt the document IDs to match your dataset.
fn load_cities(
    firestore: &Firestore,
    client: &FirestoreClient,
) -> FirestoreResult<Vec<BTreeMap<String, FirestoreValue>>> {
    // In the Modular JS SDK quickstart the sample lists a handful of known city documents.
    // We follow the same approach here. Replace the identifiers with those present in your
    // database or fetch them dynamically once collection queries are implemented over HTTP.
    let document_ids = ["la", "sf", "tokyo"];

    let mut documents = Vec::new();
    for doc_id in document_ids {
        let doc_ref = firestore.collection("cities")?.doc(Some(doc_id))?;
        let path = doc_ref.path().canonical_string();
        let snapshot = client.get_doc(path.as_str())?;
        if let Some(data) = snapshot.data() {
            documents.push(data.clone());
        }
    }

    Ok(documents)
}

fn field_as_string(data: &BTreeMap<String, FirestoreValue>, field: &str) -> Option<String> {
    data.get(field).and_then(|value| match value.kind() {
        ValueKind::String(text) => Some(text.clone()),
        _ => None,
    })
}

fn field_as_i64(data: &BTreeMap<String, FirestoreValue>, field: &str) -> Option<i64> {
    data.get(field).and_then(|value| match value.kind() {
        ValueKind::Integer(value) => Some(*value),
        _ => None,
    })
}
