# Firebase rs SDK Unofficial

This is a unofficial port of the Google's Firebase JS SDK. The goal is to mirror the features offered by the JavaScript SDK while exposing idiomatic Rust APIs.

**A note from the author:** Firebase was launched in 2011 and later acquired by Google. It's a shame that in 2025 there is still no official Rust SDK from Google to use the service. This is an attempt to fill that void, hoping that Google will soon create an official Rust SDK, either by starting with this library or from scratch.

This library is mainly **AI generated**. It is built by milking my ChatGPT subscription for all it's worth. I tried to instruct ChatGPT to adhere as much as possible to the original API structure and naming, so that the official JS SDK documentation could be a source of information useful also for the Rust SDK. 

At the time of writing (15th October 2025) some modules (Firestore database, Storage) are quite complete and already in use. Some other modules (App, App Check, Auth) are mainly developed and ready to use, but have not been tested by me. Other module (Ai, analytics...) only the basic functions are ported, and I am waiting to have ChatGPT resources to finish them. For all the modules there is an attempt to document the API and to port also the tests. All the code published passes `cargo test`. Beware that we still need to check if all the relevant tests have been ported from the JS SDK, and if the tests cover all the important aspects of the library.

If you want to contribute, your AI resources are the best thing you can donate to this project. See the [`CONTRIBUTING.md`] page on how to help.

**Why the JS SDK as a source?** Firebase has several official SDKs. From the point of view of the language architecture, the cpp version was probably a better source, but the JS was one of the few that implemented the services from scratch without relying on some Java external library. Besides, it is one of the most complete and best documented APIs. 

Resources for the Firebase JS SDK:

- Quickstart Guide: <https://firebase.google.com/docs/web/setup>
- API references: <https://firebase.google.com/docs/reference/js/>
- SDK Github repo: <https://github.com/firebase/firebase-js-sdk>

This material is from Google and the Community.

## Modules

As of the day of writing (October 21th, 2025), 7 modules have been ported to an extent that they can be considered stable: the main features are ported, the API calls documented, the code is tested and working examples are provided. The other modules are a stub with just basic features, and API calls could evolve significantly:

| Module | Completed | |
|--------|------------|-|
| [app](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/app)                     | 60% | `[############        ]` |
| [storage](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/storage)             | 60% | `[############        ]` |
| [installations](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/installations) | 35% | `[#######             ]` |
| [database](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/database)           | 30% | `[######              ]` |
| [auth](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/auth)                   | 25% | `[#####               ]` |
| [firestore](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/firestore)         | 25% | `[#####               ]` |
| [remote-config](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/remote_config) | 25% | `[#####               ]` |
| [analytics](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/analytics)         | 20% | `[####                ]` |
| [app_check](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/app_check)         | 20% | `[####                ]` |
| [ai](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/ai)                       | 5%  | `[#                   ]` |
| [data-connect](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/data_connect)   | 5%  | `[#                   ]` |
| [functions](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/functions)         | 5%  | `[#                   ]` |
| [messaging](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/messaging)         | 3%  | `[#                   ]` |
| [performance](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/performance)     | 3%  | `[#                   ]` |


The following modules are used internally by the library but with no direct API exposure (only the relevant features are ported on a need basis):

- [component](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/component)
- [logger](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/logger)
- [util](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/util)

## Evolution, breaking changes, breaking changes

The plan for this library is the following:

- Version 0.xx: some of the modules are partially developed
- Version 1.xx: all the modules are at a mature stage of development
- Version 2.xx: the API interface is rationalized and the tests are checked to cover the fundamental aspects of the library

For the mature modules (Auth, Firestore, Storage), we do not expect breaking changes between our current version and the 1.xx, but it is possibile that from the 1.xx to the 2.xx there will be breaking changes due to renaming and reorganizing the public API. 

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

Here is the Rust version with the ported Rust SDK:

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

As you can see, there are similarities between the Typescript methods (initializeApp(), getFirestore(), collection(), getDocs()) and the corresponding Rust methods (initialize_app(), get_firestore(), collection(), get_docs()). 

For further details, refer to the `./examples/firestore_select_documents.rs` or run `cargo run --example firestore_select_documents`.

## Copyright

The Firebase JS SDK is property of Google and is licensed under the Apache License, Version 2.0. This library does not contain any work from that library, and it is licensed under the Apache License, Version 2.0.

Please be aware that this library is distributed "as is", and the author(s) offer no guarantees, nor warranties or conditions of any kind.

## How to contribute

We are happy to accept everybody's contribution. The porting process is time and AI intensive, if you have any or both of those, your help is appreciated! Please refer to the [`CONTRIBUTING.md`] for the details. 

