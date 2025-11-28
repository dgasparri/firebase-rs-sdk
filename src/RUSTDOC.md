# Firebase rs SDK Unofficial

This is an unofficial Rust SDK for Firebase.

## Modules

The Firebase Rust SDK includes 14 modules, each mapping to a Firebase service:

| Module                                                                                    |
|-------------------------------------------------------------------------------------------|
| [ai]                       |
| [analytics]         |
| [app]                     |
| [app_check]         |
| [auth]                   |
| [data_connect]   |
| [database]           |
| [firestore]         |
| [functions]         |
| [installations] |
| [messaging]         |
| [performance]     |
| [remote_config] |
| [storage]             |

The following modules are used internally by the library and have no direct public API.

- [component]
- [logger]
- [platform]
- [util]

Note that this library is provided _as is_. Even the more mature modules have not been exhaustively tested. All the code published passes `cargo test`. There is an effort to port the tests of the official JavaScript SDK, but there is no guarantee that the test coverage is complete.

## Feature Flags

This library is WASM compatible and offers the following cargo features:

- `wasm-web`: enables the bindings required to compile the crate for `wasm32-unknown-unknown` (e.g. `wasm-bindgen`, `web-sys`, `gloo-timers`). Activate this when you target the web or run wasm-specific tests.
- `experimental-indexed-db`: turns on IndexedDB-backed persistence for modules that support it (currently App Check). Without this flag, wasm builds fall back to in-memory storage while keeping the same API.

To enable those features in `Cargo.toml`:

```toml
[dependencies]
firebase-rs-sdk = { version = "X.XX", features = ["wasm-web", "experimental-indexed-db"] }
```

To build or test with these features

```bash
cargo check --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db
cargo test --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db
cargo build --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db
```

If you only need the wasm bindings and not IndexedDB persistence, omit `experimental-indexed-db` from the list.

## Connection to Google's official JavaScript SDK

Because Firebase APIs are not fully documented, we relied heavily on the official JavaScript SDK (and, to a lesser extent on the C++ SDK) to implement the functions. The JavaScript SDK appears as one of the most complete in terms of functions and calls to the service, and it is one of the few that implements the services from scratch without depending on external Java libraries. Moreover, it offers one of the most complete and well-documented APIs.

There are often direct and clear parallels between the TypeScript methods of the JS SDK (initializeApp(), getFirestore(), collection(), getDocs()) and their counterparts in this Rust SDK (initialize_app(), get_firestore(), collection(), get_docs()).

For that reason, sometimes for calls and features it might be useful to refer directly to the JavaScript SDK:

- Quickstart Guide: <https://firebase.google.com/docs/web/setup>
- API references: <https://firebase.google.com/docs/reference/js/>
- SDK Github repo: <https://github.com/firebase/firebase-js-sdk>

(These resources are maintained by Google and the community.)

If you want to contribute, donating your time and AI resources is the most valuable way to support this project. See the [`CONTRIBUTING.md`](https://github.com/dgasparri/firebase-rs-sdk/blob/main/CONTRIBUTING.md) page on how to help.

## Example

Connects to the Firestore service, populates an in-memory-only document with some mock values, and retrieves them.

```rust,no_run
use std::collections::BTreeMap;
use std::error::Error;

use firebase_rs_sdk::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::firestore::*;

# #[cfg(target_arch = "wasm32")]
# fn main() {}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let firebase_config = FirebaseOptions {
        api_key: Some("demo-api-key".into()),
        project_id: Some("demo-project".into()),
        ..Default::default()
    };

    let app = initialize_app(firebase_config, Some(FirebaseAppSettings::default())).await?;
    let firestore_arc = get_firestore(Some(app.clone())).await?;
    let firestore = Firestore::from_arc(firestore_arc);

    // Talk to the hosted Firestore REST API. Configure credentials/tokens as needed.
    let client = FirestoreClient::with_http_datastore(firestore.clone())?;

    let cities = load_cities(&firestore, &client).await?;

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

/// Mirrors the `getCities` helper in `JSEXAMPLE.ts`, issuing the equivalent modular query
/// against the remote Firestore backend.
async fn load_cities(
    firestore: &Firestore,
    client: &FirestoreClient,
) -> FirestoreResult<Vec<BTreeMap<String, FirestoreValue>>> {
    // The modular JS quickstart queries the `cities` collection.
    let query = firestore.collection("cities")?.query();
    let snapshot = client.get_docs(&query).await?;

    let mut documents = Vec::new();
    for doc in snapshot.documents() {
        if let Some(data) = doc.data() {
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
```

For further details, refer to the example [`./examples/firestore_select_documents.rs`](https://github.com/dgasparri/firebase-rs-sdk/blob/main/examples/firestore_select_documents.rs) or run `cargo run --example firestore_select_documents`.

## Copyright

This library is licensed under the Apache License, Version 2.0.

This library is distributed ‘as is’ without warranties or conditions of any kind.

## How to contribute

The porting process is time- and AI-intensive; any help is appreciated. See [`CONTRIBUTING.md`](https://github.com/dgasparri/firebase-rs-sdk/blob/main/CONTRIBUTING.md) for details.
