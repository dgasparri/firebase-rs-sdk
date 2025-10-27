# Firebase Storage Port (Rust)

## Introduction

This module ports core pieces of the Firebase Storage Web SDK to Rust so applications
can discover buckets, navigate object paths, and perform common download, metadata,
and upload operations using an async `reqwest` client that works on native and wasm targets.

It provides functionality to interact with Firebase Storage, including
uploading and downloading files, managing metadata, and handling storage references.

It includes error handling, configuration options, and integration with Firebase apps.

### Features:

- Connect to Firebase Storage emulator
- Get storage instance for a Firebase app
- Register storage component
- Manage storage references
- Handle file uploads with progress tracking
- Upload strings and browser blobs with shared helpers
- Stream large uploads directly from async readers
- List files and directories in storage
- Manage object metadata
- Comprehensive error handling

## Porting status

- storage 60% `[######    ]`

==As of October 21th, 2025== 

Prompt: Compare the original JS/Typescript files in ./packages/storage and the ported files in Rust in ./src/storage, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust for this module

Thinking
I'll create a clear, concise answer with bullet points under headers for positive coverage and key gaps, referencing specific source lines like src/storage/reference.rs:81 and packages/storage/src/api.ts:129. I’ll include an estimated completeness around 60% and a friendly next-step suggestion about porting missing features to wrap it up nicely.

Storage Port Coverage

  - Core reference operations (metadata fetch/update, listing, byte downloads, multipart & resumable uploads) are
  present in Rust (src/storage/reference.rs:81, src/storage/reference.rs:94, src/storage/reference.rs:145, src/storage/
  reference.rs:195) and backed by the request builders and retry plumbing in src/storage/request/builders.rs:1.
  - Storage instance management mirrors the JS service: component registration, emulator support, bucket resolution, and
  auth/app-check header injection live in src/storage/api.rs:12 and src/storage/service.rs:1.

Key Gaps

  - JS exposes direct `ReadableStream`/service-worker piping (packages/storage/src/reference.ts:201), whereas the Rust
  port still requires callers to adapt browser streams into `AsyncRead` sources before invoking the resumable helpers.
  - The modular upload task with observers, pause/resume/cancel, and snapshot events (packages/storage/src/task.ts:62,
  packages/storage/src/public-types.ts:214) is only partially implemented: the async `UploadTask` covers resumable flows
  but still lacks observer hooks and state transitions (src/storage/upload.rs:40).
  - Error coverage is narrower: the Rust enum covers a handful of codes (src/storage/error.rs:1), whereas the JS SDK
  handles the full suite including auth/quota/retry cases (packages/storage/src/implementation/error.ts:88), and there’s
  no token-refresh logic on 401/403 responses.
  - Estimated completion: roughly 60 % of the Firebase Storage web SDK surface has been ported to Rust—core CRUD and
  upload flows are in place, but advanced task management, streaming/string conveniences, and full error parity remain
  outstanding.

Natural next step: wire in observer-based upload tasks, add direct `ReadableStream` adapters for WASM, and continue filling
out the remaining error/streaming parity gaps.

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
use firebase_rs_sdk::app::*;
use firebase_rs_sdk::storage::*;

#[tokio::main]
async fn main() -> StorageResult<()> {
    let options = FirebaseOptions {
        storage_bucket: Some("BUCKET_NAME".into()),
        ..Default::default()
    };

    let app = initialize_app(options, Some(FirebaseAppSettings::default())).await?;

    let storage = get_storage_for_app(Some(app), None).await?;

    let photos = storage
        .root_reference()?
        .child("photos");

    // Upload a photo; small payloads are sent via multipart upload while larger blobs use the resumable API.
    let image_bytes = vec![/* PNG bytes */];
    let mut upload_metadata = UploadMetadata::new().with_content_type("image/png");
    upload_metadata.insert_custom_metadata("uploaded-by", "quickstart");

    let metadata = photos
        .child("welcome.png")
        .upload_bytes(image_bytes, Some(upload_metadata))
        .await?;
    println!(
        "Uploaded {} to bucket {}",
        metadata.name.unwrap_or_default(),
        metadata.bucket.unwrap_or_default()
    );

    // List the directory and stream the first few kilobytes of each item.
    let listing = photos.list_all().await?;
    for object in listing.items {
        let url = object.get_download_url().await?;
        let bytes = object.get_bytes(Some(256 * 1024)).await?;
        println!("{} -> {} bytes", url, bytes.len());
    }

    Ok(())
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
- Added upload support: multipart uploads expose an async `upload_bytes` helper, and resumable uploads are modelled
  through a Rust-centric `UploadTask` that streams chunks, surfaces progress callbacks, and finalises with parsed
  metadata. Request builders for multipart/resumable flows are unit-tested with emulator-style mocks.
- Implemented string uploads (`upload_string`) plus WASM conveniences for `Blob`/`Uint8Array` sources and `get_blob`
  downloads so browser callers can mirror the Web SDK entry points without extra glue code.
- Added `upload_reader_resumable` helpers so large files can stream from any `AsyncRead` without buffering the entire
  payload, keeping native and wasm behaviour aligned with the JS SDK's resumable flow.
- Expanded metadata and type models: `ObjectMetadata` now tracks MD5/CRC/ETag values, parses download tokens into a
  typed collection, and exposes helpers for byte sizes. `UploadMetadata`/`SettableMetadata` provide builder-style
  ergonomics for configuring uploads and metadata updates while serialising to the REST-friendly camelCase payloads.
- Authentication and App Check headers are now injected automatically: emulator overrides feed `Authorization`
  headers, while live environments consult the Auth/App Check providers to emit `Authorization`,
  `X-Firebase-AppCheck`, `X-Firebase-Storage-Version`, and `X-Firebase-GMPID` metadata on every request.
- Unified async transport built on `reqwest::Client`, so native and wasm targets share the same retry logic while
  exposing an `async` public API.

## Still To Do

1. **Token refresh & error awareness** – Now that headers are attached, add handling for auth/app-check failures by
   forcing token refreshes on 401/403 responses and mapping them to dedicated `StorageErrorCode`s.
2. **Browser streaming sources** – Add adapters for `ReadableStream`/service-worker streams so WASM callers can forward
  native browser streams without intermediate buffering, matching the JS SDK's Blob stream integration.
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
