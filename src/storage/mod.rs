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
//! - Core functionalities: Mostly implemented (see the module's [README](https://github.com/dgasparri/firebase-rs-sdk/tree/main/src/storage) for details)
//! - Tests: 27 tests (passed)
//! - Documentation: Lacking documentation on most functions
//! - Examples: None provided
//!
//! DISCLAIMER: This is not an official Firebase product, nor it is guaranteed that it has no bugs or that it will work as intended.
//!
//! # Example
//!
//! ```rust,no_run
//! use firebase_rs_sdk::app::*;
//! use firebase_rs_sdk::storage::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let options = FirebaseOptions {
//!         storage_bucket: Some("BUCKET_NAME".into()),
//!         ..Default::default()
//!     };
//!
//!     let app = initialize_app(options, Some(FirebaseAppSettings::default())).await?;
//!
//!     let storage = get_storage_for_app(Some(app), None).await?;
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
//!         .await?;
//!     println!(
//!         "Uploaded {} to bucket {}",
//!         metadata.name.unwrap_or_default(),
//!         metadata.bucket.unwrap_or_default()
//!     );
//!
//!     // List the directory and stream the first few kilobytes of each item.
//!     let listing = photos.list_all().await?;
//!     for object in listing.items {
//!         let url = object.get_download_url().await?;
//!         let bytes = object.get_bytes(Some(256 * 1024)).await?;
//!         println!("{} -> {} bytes", url, bytes.len());
//!     }
//!     Ok(())
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
mod stream;
mod string;
mod upload;
mod util;
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
mod wasm;

#[doc(inline)]
pub use api::{
    connect_storage_emulator, delete_storage_instance, get_storage_for_app,
    register_storage_component, storage_ref_from_reference, storage_ref_from_storage,
};

#[doc(inline)]
pub use constants::{
    DEFAULT_HOST, DEFAULT_MAX_OPERATION_RETRY_TIME_MS, DEFAULT_MAX_UPLOAD_RETRY_TIME_MS,
    DEFAULT_PROTOCOL, STORAGE_TYPE,
};

#[doc(inline)]
pub use error::{StorageError, StorageErrorCode, StorageResult};

#[doc(inline)]
pub use list::{build_list_options, parse_list_result, ListOptions, ListResult};

#[doc(inline)]
pub use location::Location;

#[doc(inline)]
pub use metadata::{ObjectMetadata, SetMetadataRequest, SettableMetadata, UploadMetadata};

#[doc(inline)]
pub use path::{child, last_component, parent};

#[doc(inline)]
pub use reference::StorageReference;

#[doc(inline)]
pub use request::{
    continue_resumable_upload_request, create_resumable_upload_request, delete_object_request,
    download_bytes_request, download_url_request, get_metadata_request,
    get_resumable_upload_status_request, list_request, multipart_upload_request,
    update_metadata_request, BackoffConfig, BackoffState, HttpClient, RequestBody, RequestError,
    RequestInfo, ResponseHandler, ResponsePayload, ResumableUploadStatus,
    RESUMABLE_UPLOAD_CHUNK_SIZE,
};
#[cfg(not(target_arch = "wasm32"))]
#[doc(inline)]
pub use request::{StorageByteStream, StreamingResponse};

// pub use request::builders::{create_resumable_upload_request,  delete_object_request, download_bytes_request, download_url_request, get_metadata_request, get_resumable_upload_status_request,  list_request, multipart_upload_request, update_metadata_request, RequestInfo, RequestMethod, RequestBuilder, ResumableUploadStatus, RESUMABLE_UPLOAD_CHUNK_SIZE};

#[doc(inline)]
pub use service::FirebaseStorageImpl;

#[doc(inline)]
pub use string::StringFormat;

#[doc(inline)]
pub use upload::{UploadProgress, UploadTask, UploadTaskState};

#[doc(inline)]
pub use util::{is_retry_status_code, is_url};

#[doc(inline)]
pub use stream::UploadAsyncRead;
