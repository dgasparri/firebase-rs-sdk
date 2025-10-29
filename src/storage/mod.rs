#![doc = include_str!("README.md")]
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
