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
    let firestore = Firestore::from_arc(firestore_arc.clone());

    // Use the in-memory datastore so the example stays self-contained.
    let client = FirestoreClient::with_in_memory(firestore.clone());

    seed_cities(&client)?;
    let cities = get_cities(&firestore, &client)?;

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

/// Mirrors the `getCities` helper in `JSEXAMPLE.ts` using the Rust APIs.
fn get_cities(
    firestore: &Firestore,
    client: &FirestoreClient,
) -> FirestoreResult<Vec<BTreeMap<String, FirestoreValue>>> {
    let query = firestore.collection("cities")?.query();
    let snapshot = client.get_docs(&query)?;

    let mut documents = Vec::new();
    for doc in snapshot.documents() {
        if let Some(data) = doc.data() {
            documents.push(data.clone());
        }
    }

    Ok(documents)
}

fn seed_cities(client: &FirestoreClient) -> FirestoreResult<()> {
    let cities = [
        ("sf", "San Francisco", "California", "USA", 860_000),
        ("la", "Los Angeles", "California", "USA", 3_980_000),
        ("tokyo", "Tokyo", "Tokyo Prefecture", "Japan", 13_960_000),
    ];

    for (id, name, state, country, population) in cities {
        let mut data = BTreeMap::new();
        data.insert("name".into(), FirestoreValue::from_string(name));
        data.insert("state".into(), FirestoreValue::from_string(state));
        data.insert("country".into(), FirestoreValue::from_string(country));
        data.insert(
            "population".into(),
            FirestoreValue::from_integer(population),
        );
        client.set_doc(&format!("cities/{id}"), data, None)?;
    }

    Ok(())
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
