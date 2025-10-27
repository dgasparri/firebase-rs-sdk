//! # Firebase Firestore module
//!
//! This module ports core pieces of the Firestore SDK to Rust so applications
//! can discover collections and create, update, retrieve and delete documents.
//!
//! It provides functionality to interact with Firestore, including retrieving and querying documents,
//! working with collections, and managing real-time updates.
//!
//! It includes error handling, configuration options, and integration with Firebase apps.
//!
//! ## Features
//!
//! - Connect to Firestore emulator
//! - Get Firestore instance for a Firebase app
//! - Register Firestore component
//! - Manage collections and documents
//! - Build and execute queries
//! - Comprehensive error handling
//!
//! ## References to the Firebase JS SDK - firestore module
//!
//! - QuickStart: <https://firebase.google.com/docs/firestore/quickstart>
//! - API: <https://firebase.google.com/docs/reference/js/firestore.md#firestore_package>
//! - Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firestore>
//! - Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/firestore>
//!
//! ## Development status as of 14th October 2025
//!
//! - Core functionalities: Mostly implemented (see the module's [README.md](https://github.com/dgasparri/firebase-rs-sdk/tree/main/src/firestore) for details)
//! - Tests: 31 tests (passed)
//! - Documentation: Most public functions are documented
//! - Examples: None provided
//!
//! DISCLAIMER: This is not an official Firebase product, nor it is guaranteed that it has no bugs or that it will work as intended.
//!
//! # Example
//!
//! ```rust,ignore
//! use firebase_rs_sdk::app::api::initialize_app;
//! use firebase_rs_sdk::app::{FirebaseAppSettings, FirebaseOptions};
//! use firebase_rs_sdk::app_check::api::{custom_provider, initialize_app_check, token_with_ttl};
//! use firebase_rs_sdk::app_check::{AppCheckOptions, FirebaseAppCheckInternal};
//! use firebase_rs_sdk::auth::api::auth_for_app;
//! use firebase_rs_sdk::firestore::*;
//! use std::collections::BTreeMap;
//! use std::time::Duration;
//!
//! #[tokio::main(flavor = "current_thread")]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // TODO: replace with your project configuration
//!     let options = FirebaseOptions {
//!         project_id: Some("your-project".into()),
//!         // add other Firebase options as needed
//!         ..Default::default()
//!     };
//!     let app = initialize_app(options, Some(FirebaseAppSettings::default())).await?;
//!     let auth = auth_for_app(app.clone())?;
//!
//!     // Optional: wire App Check tokens into Firestore.
//!     let app_check_provider = custom_provider(|| token_with_ttl("fake-token", Duration::from_secs(60)));
//!     let app_check =
//!         initialize_app_check(Some(app.clone()), AppCheckOptions::new(app_check_provider)).await?;
//!     let app_check_internal = FirebaseAppCheckInternal::new(app_check);
//!
//!     let firestore = get_firestore(Some(app.clone())).await?;
//!     let client = FirestoreClient::with_http_datastore_authenticated(
//!         firebase_rs_sdk::firestore::api::Firestore::from_arc(firestore.clone()),
//!         auth.token_provider(),
//!         Some(app_check_internal.token_provider()),
//!     )?;
//!
//!     let mut ada = BTreeMap::new();
//!     ada.insert("first".into(), FirestoreValue::from_string("Ada"));
//!     ada.insert("last".into(), FirestoreValue::from_string("Lovelace"));
//!     ada.insert("born".into(), FirestoreValue::from_integer(1815));
//!     let ada_snapshot = client.add_doc("users", ada).await?;
//!     println!("Document written with ID: {}", ada_snapshot.id());
//!
//!     let mut alan = BTreeMap::new();
//!     alan.insert("first".into(), FirestoreValue::from_string("Alan"));
//!     alan.insert("middle".into(), FirestoreValue::from_string("Mathison"));
//!     alan.insert("last".into(), FirestoreValue::from_string("Turing"));
//!     alan.insert("born".into(), FirestoreValue::from_integer(1912));
//!     let alan_snapshot = client.add_doc("users", alan).await?;
//!     println!("Document written with ID: {}", alan_snapshot.id());
//!
//!     Ok(())
//! }
//! ```
//!
//! If App Check is not enabled for your app, pass `None` as the third argument to
//! `with_http_datastore_authenticated`.
//!
//! Using Converters:
//!
//! ```rust,ignore
//! use firebase_rs_sdk::app::*;
//! use firebase_rs_sdk::firestore::*;
//! use std::collections::BTreeMap;
//!
//! #[derive(Clone)]
//! struct MyUser {
//!    name: String,
//! }
//!
//! #[derive(Clone)]
//! struct UserConverter;
//!
//! impl FirestoreDataConverter for UserConverter {
//!     type Model = MyUser;
//!
//!     fn to_map(
//!         &self,
//!         value: &Self::Model,
//!     ) -> FirestoreResult<BTreeMap<String, FirestoreValue>> {
//!         // Encode your model into Firestore fields.
//!         todo!()
//!     }
//!
//!     fn from_map(&self, value: &MapValue) -> FirestoreResult<Self::Model> {
//!         // Decode Firestore fields into your model.
//!         todo!()
//!     }
//! }
//!
//! async fn example_with_converter(
//!     firestore: &Firestore,
//!     client: &FirestoreClient,
//! ) -> FirestoreResult<Option<MyUser>> {
//!     let users = firestore.collection("typed-users")?.with_converter(UserConverter);
//!     let doc = users.doc(Some("ada"))?;
//!     client
//!         .set_doc_with_converter(&doc, MyUser { name: "Ada".to_string() }, None)
//!         .await?;
//!     let typed_snapshot = client.get_doc_with_converter(&doc).await?;
//!     let user: Option<MyUser> = typed_snapshot.data()?;
//!     Ok(user)
//! }
//! ```

pub mod api;
mod constants;
pub mod error;
pub mod model;
pub mod remote;
pub mod value;

#[doc(inline)]
pub use api::{
    get_firestore, register_firestore_component, CollectionReference, ConvertedCollectionReference,
    ConvertedDocumentReference, ConvertedQuery, DocumentReference, DocumentSnapshot,
    FilterOperator, Firestore, FirestoreClient, FirestoreDataConverter, LimitType, OrderDirection,
    PassthroughConverter, Query, QuerySnapshot, SetOptions, SnapshotMetadata,
    TypedDocumentSnapshot, TypedQuerySnapshot,
};

#[doc(inline)]
pub use api::query::QueryDefinition;

#[doc(inline)]
pub use constants::{DEFAULT_DATABASE_ID, FIRESTORE_COMPONENT_NAME};

#[doc(inline)]
pub use model::{DatabaseId, DocumentKey, FieldPath, GeoPoint, ResourcePath, Timestamp};

#[doc(inline)]
pub use remote::{
    map_http_error, Connection, ConnectionBuilder, Datastore, HttpDatastore, InMemoryDatastore,
    JsonProtoSerializer, NoopTokenProvider, RequestContext, RetrySettings, TokenProviderArc,
};

#[doc(inline)]
pub use remote::datastore::{http::HttpDatastoreBuilder, TokenProvider};

#[doc(inline)]
pub use value::{ArrayValue, BytesValue, FirestoreValue, MapValue, ValueKind};

#[doc(inline)]
pub use error::{FirestoreError, FirestoreErrorCode, FirestoreResult};
