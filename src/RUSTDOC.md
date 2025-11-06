# Firebase rs SDK Unofficial

This is an unofficial port of Google's Firebase JS SDK. The goal is to mirror the features offered by the JavaScript SDK while exposing idiomatic Rust APIs. Although Firebase was launched in 2011, Google has still not released an official Rust SDK for it. This is an attempt to fill that gap.

## Modules

As of this writing (October 21th, 2025), out of 14 modules, 9 modules have been ported to an extent that they can be considered stable: the main features are ported, the API calls are documented, the code is tested and working examples are provided. Only minor changes to the public API are expected. The remaining modules are still a work in progress, and their APIs may change significantly.


| Module | % porting completed  | |
|--------|----------------------|-|
| [ai]                       | 30% | `[######              ]` |
| [analytics]         | 20% | `[####                ]` |
| [app]                     | 80% | `[################    ]` |
| [app_check]         | 70% | `[##############      ]` |
| [auth]                   | 85% | `[#################   ]` |
| [data_connect]   | 5%  | `[#                   ]` |
| [database]           | 30% | `[######              ]` |
| [firestore]         | 85% | `[#################   ]` |
| [functions]         | 25% | `[#####               ]` |
| [installations] | 45% | `[#########           ]` |
| [messaging]         | 40% | `[########            ]` |
| [performance]     | 5%  | `[#                   ]` |
| [remote_config] | 25% | `[#####               ]` |
| [storage]             | 60% | `[############        ]` |


The following modules are used internally by the library and have no direct public API. Only the features required internally have been ported.

- [component]
- [logger]
- [platform]
- [util]



Note that this library is provided _as is__. Even the more developed modules have not yet been exhaustively tested. All the code published passes `cargo test` and the original tests of the JS SDK are being ported, but we are still verifying that all relevant tests from the JS SDK have been ported and that the test coverage is complete.


## Feature Flags

This library is WASM compatible and offers the following cargo features:

- `wasm-web`: enables the bindings required to compile the crate for `wasm32-unknown-unknown` (e.g. `wasm-bindgen`, `web-sys`, `gloo-timers`). Activate this when you target the web or run wasm-specific tests.
- `experimental-indexed-db`: turns on IndexedDB-backed persistence for modules that support it (currently App Check). Without this flag, wasm builds fall back to in-memory storage while keeping the same API.

To enable those features in `Cargo.toml`:

```toml
[dependencies]
firebase-rs-sdk = { version = "X.XX", features = ["wasm-web", "experimental-indexed-db"] }
```

To run commands with the features

```bash
cargo check --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db
cargo test --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db
cargo build --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db
```

If you only need the wasm bindings and not IndexedDB persistence, omit `experimental-indexed-db` from the list.

##  Why the JS SDK as a source?

Firebase has several official SDKs. The JS SDK is one of the few that implements the services from scratch without depending on external Java libraries. Moreover, it offers one of the most complete and well-documented APIs. 

Resources for the Firebase JS SDK:

- Quickstart Guide: <https://firebase.google.com/docs/web/setup>
- API references: <https://firebase.google.com/docs/reference/js/>
- SDK Github repo: <https://github.com/firebase/firebase-js-sdk>

This material is from Google and the Community.

There is an effort to match closely the structure and names of the JS SDK, so its documentation might be of help to understand the Rust porting library.

If you want to contribute, donating your time and AI resources is the most valuable way to support this project. See the [`CONTRIBUTING.md`](https://github.com/dgasparri/firebase-rs-sdk/blob/main/CONTRIBUTING.md) page on how to help.


# Example

This is an example provided by the Quickstart guide for the official Firebase Javascript SDK:

```ts
import { initializeApp } from 'firebase/app';
import { getFirestore, collection, getDocs } from 'firebase/firestore/lite';
// Follow this pattern to import other Firebase services
// import { } from 'firebase/<service>';

// TODO: Replace the following with your app's Firebase configuration
const firebaseConfig = {
  //...
};

const app = initializeApp(firebaseConfig);
const db = getFirestore(app);

// Get a list of cities from your database
async function getCities(db) {
  const citiesCol = collection(db, 'cities');
  const citySnapshot = await getDocs(citiesCol);
  const cityList = citySnapshot.docs.map(doc => doc.data());
  return cityList;
}
```

Here is the equivalent example using the Rust port of the SDK:

```rust,no_run
use std::collections::BTreeMap;
use std::error::Error;

use firebase_rs_sdk::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::firestore::*;

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

There are clear parallels between the TypeScript methods (initializeApp(), getFirestore(), collection(), getDocs()) and their Rust counterparts (initialize_app(), get_firestore(), collection(), get_docs()). 

For further details, refer to the example [`./examples/firestore_select_documents.rs`](https://github.com/dgasparri/firebase-rs-sdk/blob/main/examples/firestore_select_documents.rs) or run `cargo run --example firestore_select_documents`.

## Copyright

The Firebase JS SDK is the property of Google and is licensed under the Apache License, Version 2.0. This library does not contain any code from that SDK, and it is licensed under the Apache License, Version 2.0.

Please be aware that this library is distributed "as is", and the author(s) offer no guarantees, nor warranties or conditions of any kind.

## How to contribute

We welcome contributions from everyone. The porting process is time and AI intensive, if you have any or both of those, your help is appreciated! Please refer to the [`CONTRIBUTING.md`](https://github.com/dgasparri/firebase-rs-sdk/blob/main/CONTRIBUTING.md) for the details. 
