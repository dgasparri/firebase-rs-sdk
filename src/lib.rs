//! # Firebase rs SDK Unofficial
//!
//! This is a unofficial port of the Google's Firebase JS SDK. The goal is to mirror the features offered by the JavaScript SDK while exposing idiomatic Rust APIs.
//!
//! This library is mainly **AI generated**. We instructed ChatGPT to adhere as much as possible to the original API structure and naming, so that the official JS SDK documentation could be a source of information useful also for the Rust SDK.
//!
//! At the time of writing (15th October 2025) some modules (Firestore database, Storage) are quite complete and already in use. Some other modules (App, App Check, Auth) are mainly developed and ready to use, but have not been tested by me. Other module (Ai, analytics...) are still in the process to being ported.
//!
//! For all the modules there is an attempt to document the API and to port also the tests. All the code published passes `cargo test`. Beware that we still need to check if all the relevant tests have been ported from the JS SDK, and if the tests cover all the important aspects of the library.
//!
//! Resources for the Firebase JS SDK:
//!
//! - Quickstart Guide: <https://firebase.google.com/docs/web/setup>
//! - API references: <https://firebase.google.com/docs/reference/js/>
//! - SDK Github repo: <https://github.com/firebase/firebase-js-sdk>
//!
//! This material is from Google and the Community.
//!
//! ## Modules
//!
//! Stable/fully ported (vast majority of features and tests ported, API calls are documented, there are working examples):
//!
//! - [app](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/app)
//! - [auth](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/auth)
//! - [auth_check](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/auth_check)
//! - [database](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/database)
//! - [firestore](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/firestore)
//! - [storage](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/storage)
//!
//! Minimal porting (some basic features and tests are ported, some API call documentation is missing, there is no working , API calls could evolve significantly):
//!
//! - [ai](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/ai)
//! - [analytics](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/analytics)
//! - [app_check](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/app_check)
//! - [data-connect](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/data_connect)
//! - [functions](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/functions)
//! - [installations](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/installations)
//! - [messaging](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/messaging)
//! - [performance](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/performance)
//! - [remote-config](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/remote_config)
//!
//!
//! Modules used internally by the library but with no direct API exposure (only the relevant features are ported on a need basis):
//!
//! - [component](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/component)
//! - [logger](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/logger)
//! - [util](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/util)
//!
//! # Example
//!
//! This is a Typescript example provided by the Quickstart guide for the official Firebase Javascript SDK:
//!
//! ```ts
//! import { initializeApp } from 'firebase/app';
//! import { getFirestore, collection, getDocs } from 'firebase/firestore/lite';
//! // Follow this pattern to import other Firebase services
//! // import { } from 'firebase/<service>';
//!
//! // TODO: Replace the following with your app's Firebase configuration
//! const firebaseConfig = {
//!   //...
//! };
//!
//! const app = initializeApp(firebaseConfig);
//! const db = getFirestore(app);
//!
//! // Get a list of cities from your database
//! async function getCities(db) {
//!   const citiesCol = collection(db, 'cities');
//!   const citySnapshot = await getDocs(citiesCol);
//!   const cityList = citySnapshot.docs.map(doc => doc.data());
//!   return cityList;
//! }
//! ```
//!
//! Here is the Rust version with the ported Rust SDK:
//!
//! ```rust,no_run
//! use std::collections::BTreeMap;
//! use std::error::Error;
//!
//! use firebase_rs_sdk_unofficial::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};
//! use firebase_rs_sdk_unofficial::firestore::*;
//!
//! fn main() -> Result<(), Box<dyn Error>> {
//!     let firebase_config = FirebaseOptions {
//!         api_key: Some("demo-api-key".into()),
//!         project_id: Some("demo-project".into()),
//!         ..Default::default()
//!     };
//!
//!     let app = initialize_app(firebase_config, Some(FirebaseAppSettings::default()))?;
//!     let firestore_arc = get_firestore(Some(app.clone()))?;
//!     let firestore = Firestore::from_arc(firestore_arc);
//!
//!     // Talk to the hosted Firestore REST API. Configure credentials/tokens as needed.
//!     let client = FirestoreClient::with_http_datastore(firestore.clone())?;
//!
//!     let cities = load_cities(&firestore, &client)?;
//!
//!     println!("Loaded {} cities from Firestore:", cities.len());
//!     for city in cities {
//!         let name = field_as_string(&city, "name").unwrap_or_else(|| "Unknown".into());
//!         let state = field_as_string(&city, "state").unwrap_or_else(|| "Unknown".into());
//!         let country = field_as_string(&city, "country").unwrap_or_else(|| "Unknown".into());
//!         let population = field_as_i64(&city, "population").unwrap_or_default();
//!         println!("- {name}, {state} ({country}) — population {population}");
//!     }
//!
//!     Ok(())
//! }
//!
//! /// Mirrors the `getCities` helper in `JSEXAMPLE.ts`, issuing the equivalent modular query
//! /// against the remote Firestore backend.
//! fn load_cities(
//!     firestore: &Firestore,
//!     client: &FirestoreClient,
//! ) -> FirestoreResult<Vec<BTreeMap<String, FirestoreValue>>> {
//!     // The modular JS quickstart queries the `cities` collection.
//!     let query = firestore.collection("cities")?.query();
//!     let snapshot = client.get_docs(&query)?;
//!
//!     let mut documents = Vec::new();
//!     for doc in snapshot.documents() {
//!         if let Some(data) = doc.data() {
//!             documents.push(data.clone());
//!         }
//!     }
//!
//!     Ok(documents)
//! }
//!
//! fn field_as_string(data: &BTreeMap<String, FirestoreValue>, field: &str) -> Option<String> {
//!     data.get(field).and_then(|value| match value.kind() {
//!         ValueKind::String(text) => Some(text.clone()),
//!         _ => None,
//!     })
//! }
//!
//! fn field_as_i64(data: &BTreeMap<String, FirestoreValue>, field: &str) -> Option<i64> {
//!     data.get(field).and_then(|value| match value.kind() {
//!         ValueKind::Integer(value) => Some(*value),
//!         _ => None,
//!     })
//! }
//! ```
//!
//! The name of methods and data types has been kept similar between the two languages:
//!  - Typescript methods: initializeApp(), getFirestore(), collection(), getDocs()
//!  - Rust methods: initialize_app(), get_firestore(), collection(), get_docs().
//!
//! For further details, refer to the examples in the `./examples` folder. Examples can be run with `cargo run --example <example_name>`.
//!
//! ## Copyright
//!
//! The Firebase JS SDK is property of Google and is licensed under the Apache License, Version 2.0. This library does not contain any work from that library, and it is licensed under the Apache License, Version 2.0.
//!
//! Please be aware that this library is distributed "as is", and the author(s) offer no guarantees, nor warranties or conditions of any kind.

pub mod ai;
pub mod analytics;
pub mod app;
pub mod app_check;
pub mod auth;
pub mod component;
pub mod data_connect;
pub mod database;
pub mod firestore;
pub mod functions;
pub mod installations;
pub mod logger;
pub mod messaging;
pub mod performance;
pub mod remote_config;
pub mod storage;
pub mod util;

#[cfg(test)]
pub mod test_support;
