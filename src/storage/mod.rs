//! # Firebase Storage module.
//! 
//! This module ports core pieces of the Firebase Storage Web SDK to Rust so applications 
//! can discover buckets, navigate object paths, and perform common download, metadata, 
//! and upload operations in a synchronous, `reqwest`-powered environment.
//! 
//! It provides functionality to interact with Firebase Storage, including
//! uploading and downloading files, managing metadata, and handling storage references.
//! 
//! It includes error handling, configuration options, and integration with Firebase apps.
//! 
//! ## Features
//! 
//! - Connect to Firebase Storage emulator
//! - Get storage instance for a Firebase app
//! - Register storage component
//! - Manage storage references
//! - Handle file uploads with progress tracking
//! - List files and directories in storage
//! - Manage object metadata
//! - Comprehensive error handling
//! 
//! ## References to the Firebase JS SDK - storage module
//! 
//! - QuickStart: <https://firebase.google.com/docs/storage/web/start>
//! - API: <https://firebase.google.com/docs/reference/js/storage.md#storage_package>
//! - Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/storage>
//! - Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/storage>
//! 
//! ## Development status as of 14th October 2025
//! 
//! - Core functionalities: Mostly implemented (see the module's [README](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/storage) for details)
//! - Tests: 27 tests (passed)
//! - Documentation: Lacking documentation on most functions
//! - Examples: None provided
//! 
//! DISCLAIMER: This is not an official Firebase product, nor it is guaranteed that it has no bugs or that it will work as intended.
//! 
//! # Example
//! 
//! ```rust,no_run
//! use firebase_rs_sdk_unofficial::app::api::initialize_app;
//! use firebase_rs_sdk_unofficial::app::{FirebaseAppSettings, FirebaseOptions};
//! use firebase_rs_sdk_unofficial::storage::{get_storage_for_app, UploadMetadata};
//! 
//! fn main() {
//!     let options = FirebaseOptions {
//!         storage_bucket: Some("BUCKET_NAME".into()),
//!         ..Default::default()
//!     };
//! 
//!     let app = initialize_app(options, Some(FirebaseAppSettings::default()))
//!         .expect("failed to initialize app");
//! 
//!     let storage = get_storage_for_app(Some(app), None)
//!         .expect("storage component not available");
//! 
//!     let photos = storage
//!         .root_reference()
//!         .expect("missing default bucket")
//!         .child("photos");
//! 
//!     // Upload a photo; small payloads are sent via multipart upload while larger blobs use the resumable API.
//!     let image_bytes = vec![/* PNG bytes */];
//!     let mut upload_metadata = UploadMetadata::new().with_content_type("image/png");
//!     upload_metadata.insert_custom_metadata("uploaded-by", "quickstart");
//! 
//!     let metadata = photos
//!         .child("welcome.png")
//!         .upload_bytes(image_bytes, Some(upload_metadata))
//!         .expect("upload failed");
//!     println!("Uploaded {} to bucket {}", metadata.name.unwrap_or_default(), metadata.bucket.unwrap_or_default());
//! 
//!     // List the directory and stream the first few kilobytes of each item.
//!     let listing = photos.list_all().expect("failed to list objects");
//!     for object in listing.items {
//!         let url = object.get_download_url().expect("missing download URL");
//!         let bytes = object
//!             .get_bytes(Some(256 * 1024))
//!             .expect("download limited to 256 KiB");
//!         println!("{} -> {} bytes", url, bytes.len());
//!     }
//! }
//! ```


pub mod api;
mod constants;
pub mod error;
mod list;
mod location;
mod metadata;
mod path;
pub mod reference;
pub mod request;
pub mod service;
mod upload;
mod util;

pub use api::{
    connect_storage_emulator, get_storage_for_app, register_storage_component,
    storage_ref_from_reference, storage_ref_from_storage,
};
pub use constants::STORAGE_TYPE;
pub use error::{
    internal_error, invalid_argument, invalid_default_bucket, invalid_root_operation, invalid_url,
    no_default_bucket, no_download_url, unsupported_environment, StorageError, StorageErrorCode,
    StorageResult,
};
pub use list::{ListOptions, ListResult};
pub use location::Location;
pub use metadata::{ObjectMetadata, SetMetadataRequest, SettableMetadata, UploadMetadata};
pub use reference::StorageReference;
pub use service::FirebaseStorageImpl;
pub use upload::{UploadProgress, UploadTask, UploadTaskState};
