# Firebase rs SDK Unofficial

This is an unofficial port of Google's Firebase JS SDK. The goal is to mirror the features offered by the JavaScript SDK while exposing idiomatic Rust APIs. Although Firebase was launched in 2011, Google has still not released an official Rust SDK for it. This is an attempt to fill that gap.

## Modules

As of this writing (October 21th, 2025), out of 14 modules, 9 modules have been ported to an extent that they can be considered stable: the main features are ported, the API calls are documented, the code is tested and working examples are provided. Only minor changes to the public API are expected. The remaining modules are still a work in progress, and their APIs may change significantly.


| Module | % porting completed  | |
|--------|----------------------|-|
| [app](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/app)                     | 60% | `[############        ]` |
| [storage](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/storage)             | 60% | `[############        ]` |
| [installations](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/installations) | 35% | `[#######             ]` |
| [ai](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/ai)                       | 30% | `[######              ]` |
| [database](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/database)           | 30% | `[######              ]` |
| [auth](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/auth)                   | 25% | `[#####               ]` |
| [firestore](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/firestore)         | 25% | `[#####               ]` |
| [remote-config](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/remote_config) | 25% | `[#####               ]` |
| [analytics](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/analytics)         | 20% | `[####                ]` |
| [app_check](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/app_check)         | 20% | `[####                ]` |
| [data-connect](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/data_connect)   | 5%  | `[#                   ]` |
| [functions](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/functions)         | 5%  | `[#                   ]` |
| [messaging](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/messaging)         | 3%  | `[#                   ]` |
| [performance](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/performance)     | 3%  | `[#                   ]` |


The following modules are used internally by the library and have no direct public API. Only the features required internally have been ported.

- [component](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/component)
- [logger](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/logger)
- [util](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/util)



Note that this library is provided _as is__. Even the more developed modules have not yet been exhaustively tested. All the code published passes `cargo test` and the original tests of the JS SDK are being ported, but we are still verifying that all relevant tests from the JS SDK have been ported and that the test coverage is complete.

If you want to contribute, donating your time and AI resources is the most valuable way to support this project. See the [`CONTRIBUTING.md`](https://github.com/dgasparri/firebase-rs-sdk-unofficial/blob/main/CONTRIBUTING.md) page on how to help.

##  Why the JS SDK as a source?

Firebase has several official SDKs. From an architectural standpoint, the C++ version might have been a better reference, but the JS SDK is one of the few that implements the services from scratch, without depending on external Java libraries. Moreover, it offers one of the most complete and well-documented APIs. 

Resources for the Firebase JS SDK:

- Quickstart Guide: <https://firebase.google.com/docs/web/setup>
- API references: <https://firebase.google.com/docs/reference/js/>
- SDK Github repo: <https://github.com/firebase/firebase-js-sdk>

This material is from Google and the Community.

There is an effort to match closely the structure and names of the JS SDK, so its documentation might be of help to understand the Rust porting library.

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

use firebase_rs_sdk_unofficial::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk_unofficial::firestore::*;

fn main() -> Result<(), Box<dyn Error>> {
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

/// Mirrors the `getCities` helper in `JSEXAMPLE.ts`, issuing the equivalent modular query
/// against the remote Firestore backend.
fn load_cities(
    firestore: &Firestore,
    client: &FirestoreClient,
) -> FirestoreResult<Vec<BTreeMap<String, FirestoreValue>>> {
    // The modular JS quickstart queries the `cities` collection.
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

As you can see, there are clear parallels between the TypeScript methods (initializeApp(), getFirestore(), collection(), getDocs()) and their Rust counterparts (initialize_app(), get_firestore(), collection(), get_docs()). 

For further details, refer to the example [`./examples/firestore_select_documents.rs`](https://github.com/dgasparri/firebase-rs-sdk-unofficial/blob/main/examples/firestore_select_documents.rs) or run `cargo run --example firestore_select_documents`.

## Copyright

The Firebase JS SDK is the property of Google and is licensed under the Apache License, Version 2.0. This library does not contain any code from that SDK, and it is licensed under the Apache License, Version 2.0.

Please be aware that this library is distributed "as is", and the author(s) offer no guarantees, nor warranties or conditions of any kind.

## How to contribute

We welcome contributions from everyone. The porting process is time and AI intensive, if you have any or both of those, your help is appreciated! Please refer to the [`CONTRIBUTING.md`](https://github.com/dgasparri/firebase-rs-sdk-unofficial/blob/main/CONTRIBUTING.md) for the details. 

