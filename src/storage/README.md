# Firebase Storage Port (Rust)

This module ports core pieces of the Firebase Storage Web SDK to Rust so applications can discover buckets, navigate
object paths, and perform common download and metadata operations in a synchronous, `reqwest`-powered environment.

## Quick Start Example

```rust
use firebase_rs_sdk_unofficial::app::api::initialize_app;
use firebase_rs_sdk_unofficial::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk_unofficial::storage::get_storage_for_app;

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

## Still To Do

1. **Auth/App Check plumbing** – `FirebaseStorageImpl` receives the providers, but token retrieval is unimplemented.
   Inject async token fetchers and thread them into request execution so authenticated requests and emulator overrides
   respect user/app-check state.
2. **Upload flows** – Port multipart and resumable upload builders plus the `UploadTask` state machine so clients can
   mirror `uploadBytes`, `uploadBytesResumable`, and `uploadString`.
3. **Metadata & type models** – Expand the metadata module to mirror the JS SDK’s rich types (`public-types.ts`),
   including custom metadata maps, observers, and request payload helpers.
4. **Error parity** – Flesh out the error module with the full suite of error codes, HTTP status mapping, and helper
   constructors to match the TS SDK.
5. **Testing** – Broaden coverage with request-layer mocks, emulator integration smoke tests, and regression suites for
   the new operations.

## Next steps - Detailed completion plan

1. **Multipart/resumable uploads**
   - Port the multipart and resumable upload request builders (`multipartUpload`, `createResumableUpload`,
     `continueResumableUpload`) and wire them through `FirebaseStorageImpl::upload_http_client`.
   - Model the upload state machine in Rust (initial start, chunked progress, finalize) and expose it via a Rust-friendly
     `UploadTask` API that emits progress callbacks.
   - Add targeted unit tests that validate chunk boundaries and error handling using mocked HTTP responses.
2. **Authentication tokens & headers**
   - Implement token fetchers for Auth and App Check providers and inject them into request execution, ensuring headers
     are attached and refreshed when requests retry.
   - Extend the retry/error handling logic to recognise auth-specific status codes and bubble up meaningful
     `StorageErrorCode`s.
3. **Expanded metadata surface**
   - Mirror the remaining metadata mappings (custom metadata serialization, MD5/ETag fields) and expose strongly typed
     setters/getters so upload APIs can populate metadata without manual JSON handling.
   - Document the new types in rustdoc and back them with serde-based (de)serialization tests to maintain parity with
     the Web SDK.
