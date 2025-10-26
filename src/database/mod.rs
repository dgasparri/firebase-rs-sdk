//! # Firebase Realtime Database module
//!
//! This module ports core pieces of the Realtime Database from the Firebase JS SDK to Rust.
//!
//! It wires the Database component into the `FirebaseApp`, provides an in-memory backend for quick tests, and can fall back to the REST API for basic reads and writes against an emulator or hosted backend.
//!
//! Live streaming connections and the richer reference/query surface from the JS SDK are still pending.
//!
//! It includes error handling, configuration options, and integration with Firebase apps.
//!
//! ## Features
//!
//! - Component registration and shared get_database resolution
//! - Reference CRUD with auto-ID push and path navigation (parent/root)
//! - Priority-aware writes plus server value helpers (server_timestamp increment)
//! - Snapshot traversal (child, has_child, size, to_json) and value/child listeners
//! - Dual backends (in-memory + REST) with unit test coverage
//!
//!
//! ## References to the Firebase JS SDK - firestore module
//!
//! - QuickStart: <https://firebase.google.com/docs/database/web/start>
//! - API: <https://firebase.google.com/docs/reference/js/database.md#firestore_package>
//! - Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/database>
//! - Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/database>
//!
//! ## Development status as of 14th October 2025
//!
//! - Core functionalities: Mostly implemented (see the module's [README.md](https://github.com/dgasparri/firebase-rs-sdk/tree/main/src/firestore) for details)
//! - Testing: 30 tests (passed)
//! - Documentation: Most public functions are documented
//! - Examples: 2 examples
//!
//! DISCLAIMER: This is not an official Firebase product, nor it is guaranteed that it has no bugs or that it will work as intended.
//!
//!
//! ## Quick Start Example
//!
//! ```no_run
//! use firebase_rs_sdk::app::*;
//! use firebase_rs_sdk::database::{*, query as compose_query};
//! use serde_json::json;
//!
//! #[tokio::main(flavor = "current_thread")]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Point to the Realtime Database emulator or a database URL.
//!     let options = FirebaseOptions {
//!         project_id: Some("demo-project".into()),
//!         database_url: Some("http://127.0.0.1:9000/?ns=demo".into()),
//!         ..Default::default()
//!     };
//!     let app = initialize_app(options, Some(FirebaseAppSettings::default())).await?;
//!     let database = get_database(Some(app)).await?;
//!
//!     let messages = database.reference("/messages")?;
//!     messages.set(json!({ "greeting": "hello" })).await?;
//!     let value = messages.get().await?;
//!     assert_eq!(value, json!({ "greeting": "hello" }));
//!
//!     let recent = compose_query(
//!         messages,
//!         vec![order_by_child("timestamp"), limit_to_last(10)],
//!     )?;
//!     let latest = recent.get().await?;
//!     println!("latest snapshot: {latest}");
//!
//!     Ok(())
//! }
//! ```

mod api;
mod backend;
mod constants;
pub mod error;
mod on_disconnect;
mod push_id;
mod query;
mod realtime;
mod server_value;

#[doc(inline)]
pub use api::{
    end_at, end_at_with_key, end_before, end_before_with_key, equal_to, equal_to_with_key,
    get_database, limit_to_first, limit_to_last, on_child_added, on_child_changed,
    on_child_removed, order_by_child, order_by_key, order_by_priority, order_by_value, push,
    push_with_value, query, register_database_component, run_transaction, set_priority,
    set_with_priority, start_after, start_after_with_key, start_at, start_at_with_key, ChildEvent,
    ChildEventType, DataSnapshot, Database, DatabaseQuery, DatabaseReference, ListenerRegistration,
    QueryConstraint,
};

#[doc(inline)]
pub use error::DatabaseResult;

#[doc(inline)]
pub use on_disconnect::OnDisconnect;

#[doc(inline)]
pub use server_value::{increment, server_timestamp};
