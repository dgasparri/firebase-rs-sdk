# Firebase Storage Port (Rust)

## Introduction

This module ports core pieces of the Firebase Storage Web SDK to Rust so applications 
can discover buckets, navigate object paths, and perform common download, metadata, 
and upload operations in a synchronous, `reqwest`-powered environment.

It provides functionality to interact with Firebase Storage, including
uploading and downloading files, managing metadata, and handling storage references.

It includes error handling, configuration options, and integration with Firebase apps.

### Features:

- Connect to Firebase Storage emulator
- Get storage instance for a Firebase app
- Register storage component
- Manage storage references
- Handle file uploads with progress tracking
- List files and directories in storage
- Manage object metadata
- Comprehensive error handling

### References to the Firebase JS SDK - storage module

- QuickStart: https://firebase.google.com/docs/storage/web/start
- API: https://firebase.google.com/docs/reference/js/storage.md#storage_package
- Github Repo - Module: https://github.com/firebase/firebase-js-sdk/tree/master/packages/storage
- Github Repo - API: 

### Development status as of 14th October 2025

- Core functionalities: Mostly implemented 
- Tests: 27 tests (passed)
- Documentation: Lacking documentation on most functions
- Examples: None provided

DISCLAIMER: This is not an official Firebase product, nor it is guaranteed that it has no bugs or that it will work as intended.

## Quick Start Example

```rust
use firebase_rs_sdk_unofficial::app::api::initialize_app;
use firebase_rs_sdk_unofficial::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk_unofficial::storage::{get_storage_for_app, UploadMetadata};

fn main() {
    let options = FirebaseOptions {
        storage_bucket: Some("BUCKET_NAME".into()),
        ..Default::default()
    };

    let app = initialize_app(options, Some(FirebaseAppSettings::default()))
        .expect("failed to initialize app");

    let storage = get_storage_for_app(Some(app), None)
        .expect("storage component not available");

    let photos = storage
        .root_reference()
        .expect("missing default bucket")
        .child("photos");

    // Upload a photo; small payloads are sent via multipart upload while larger blobs use the resumable API.
    let image_bytes = vec![/* PNG bytes */];
    let mut upload_metadata = UploadMetadata::new().with_content_type("image/png");
    upload_metadata.insert_custom_metadata("uploaded-by", "quickstart");

    let metadata = photos
        .child("welcome.png")
        .upload_bytes(image_bytes, Some(upload_metadata))
        .expect("upload failed");
    println!("Uploaded {} to bucket {}", metadata.name.unwrap_or_default(), metadata.bucket.unwrap_or_default());

    // List the directory and stream the first few kilobytes of each item.
    let listing = photos.list_all().expect("failed to list objects");
    for object in listing.items {
        let url = object.get_download_url().expect("missing download URL");
        let bytes = object
            .get_bytes(Some(256 * 1024))
            .expect("download limited to 256 KiB");
        println!("{} -> {} bytes", url, bytes.len());
    }
}
```

## Implemented

- Registered a `storage` component so apps can lazily request Storage instances, optionally keyed by bucket URL.
- Ported the location/URL parsing helpers (`Location`, path utilities, and URL detection) with unit tests that mirror the
  JavaScript behaviour for `gs://` and HTTPS endpoints.
- Stubbed a `FirebaseStorageImpl` that tracks host/bucket state, supports emulator connection, and can produce typed
  `StorageReference` values for arbitrary child paths.
- Mirrored the public Rust API façade with helpers that wrap the component container (`get_storage_for_app`,
  `storage_ref_from_storage`, etc.).
- Introduced the request/backoff scaffolding (`storage::request`) and convenience constructors on
  `FirebaseStorageImpl` so higher layers can issue HTTP calls with exponential retry policy.
- Ported the core `StorageReference` operations: metadata fetch/update, hierarchical listing (`list`/`list_all`), direct
  downloads via `get_bytes`, signed URL generation (`get_download_url`), and object deletion. Corresponding request
  builders now emit byte-download, download-URL, and delete requests with unit coverage.
- Added upload support: multipart uploads expose a synchronous `upload_bytes` helper, and resumable uploads are modelled
  through a Rust-centric `UploadTask` that streams chunks, surfaces progress callbacks, and finalises with parsed
  metadata. Request builders for multipart/resumable flows are unit-tested with emulator-style mocks.
- Expanded metadata and type models: `ObjectMetadata` now tracks MD5/CRC/ETag values, parses download tokens into a
  typed collection, and exposes helpers for byte sizes. `UploadMetadata`/`SettableMetadata` provide builder-style
  ergonomics for configuring uploads and metadata updates while serialising to the REST-friendly camelCase payloads.
- Authentication and App Check headers are now injected automatically: emulator overrides feed `Authorization`
  headers, while live environments consult the Auth/App Check providers to emit `Authorization`,
  `X-Firebase-AppCheck`, `X-Firebase-Storage-Version`, and `X-Firebase-GMPID` metadata on every request.

## Still To Do

1. **Token refresh & error awareness** – Now that headers are attached, add handling for auth/app-check failures by
   forcing token refreshes on 401/403 responses and mapping them to dedicated `StorageErrorCode`s.
2. **String/stream uploads** – Add helpers for `upload_string`, streaming uploads, and byte-range resumptions so the API
   mirrors the JS surface for textual and streaming sources.
3. **Task observers & snapshots** – Model `UploadTaskSnapshot`, observer callbacks, and state transitions so clients can
   subscribe to upload progress events the same way the Web SDK exposes `state_changed` streams.
4. **Error parity** – Flesh out the error module with the full suite of error codes, HTTP status mapping, and helper
   constructors to match the TS SDK.
5. **Testing** – Broaden coverage with request-layer mocks, emulator integration smoke tests, and regression suites for
   the new operations.

## Next steps - Detailed completion plan

1. **Auth/App Check resiliency**
   - Teach the request pipeline to invalidate and refresh tokens when the backend returns 401/403 or the emulator hints
     at auth issues.
   - Surface distinct storage error codes for auth/app-check failures so callers can prompt users to reauthenticate.
   - When server-app support lands, read `FirebaseServerAppSettings` overrides to honour pre-provisioned tokens.
2. **Upload ergonomics**
   - Layer high-level helpers for string and stream sources on top of the new upload primitives.
   - Extend `UploadTask` with pause/resume/cancel semantics and persisted session recovery to match the JS SDK.
3. **Observer & snapshot surface**
   - Introduce `UploadTaskSnapshot` plus `StorageObserver` types that mirror the Web SDK, including typed progress
     metrics and error propagation.
   - Add unit coverage for observer registration and state transitions once snapshot modelling is in place.
