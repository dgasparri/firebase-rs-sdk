# Firebase Storage Port (Rust)

This module is the starting point for a Rust port of the Firebase Storage SDK. The current work focuses on the
foundational pieces needed for other components to obtain a storage service and navigate bucket paths.

## Implemented

- Registered a `storage` component so apps can lazily request Storage instances, optionally keyed by bucket URL.
- Ported the location/URL parsing helpers (`Location`, path utilities, and URL detection) with unit tests that mirror the
  JavaScript behaviour for `gs://` and HTTPS endpoints.
- Stubbed a `FirebaseStorageImpl` that tracks host/bucket state, supports emulator connection, and can produce typed
  `StorageReference` values for arbitrary child paths.
- Mirrored the public Rust API façade with helpers that wrap the component container (`get_storage_for_app`,
  `storage_ref_from_storage`, etc.).

These pieces are enough for other modules to obtain storage references and manipulate hierarchical paths, but they do
not yet issue network requests or expose upload/download flows.

## Still To Do

The JavaScript SDK contains far more surface area than currently ported. Major outstanding items are listed roughly in
build order:

1. **Auth/App Check plumbing** – `FirebaseStorageImpl` receives the providers, but token retrieval is unimplemented.
   Inject async token fetchers and thread them into request execution once networking is in place.
2. **Request stack** – Port the `implementation/*` helpers (connection abstractions, retry/backoff, metadata parsing,
   JSON serialization) so the service can materialise HTTP requests against the REST API and emulator.
3. **StorageReference operations** – Implement the full API (`get_bytes`, `get_metadata`, `update_metadata`,
   `list/list_all`, `get_download_url`, etc.) by delegating to the request stack. This also requires wiring upload task
   handling (`UploadTask`, resumable uploads, cancellation, observers).
4. **Error parity** – Flesh out the error module with the full suite of error codes, HTTP status mapping, and helper
   constructors to match the TS SDK.
5. **Metadata & type models** – Port `public-types.ts`, `metadata.ts`, and supporting structures so metadata/observer
   types match expectations of downstream consumers.
6. **Component integration polish** – Revisit component registration once internal components (`storage-internal`) are
   required for other services, and ensure app deletion tears down outstanding requests.
7. **Testing** – Expand unit and integration coverage once operations exist: mock request layers, confirm retry logic,
   and validate emulator wiring.

## Next Step

Begin with the **request stack** (Item 2). Implementing the low-level request primitives will unlock token usage,
metadata models, and the higher-level StorageReference APIs.

## Quick Start Example

```rust
use firebase-rs-sdk-unofficial-porting::app::api::initialize_app;
use firebase-rs-sdk-unofficial-porting::app::{FirebaseAppSettings, FirebaseOptions};
use firebase-rs-sdk-unofficial-porting::storage::get_storage_for_app;

fn main() {
    // TODO: replace with your project values
    let options = FirebaseOptions {
        storage_bucket: Some("BUCKET_NAME".into()),
        ..Default::default()
    };

    // Initialize Firebase
    let app = initialize_app(options, Some(FirebaseAppSettings::default()))
        .expect("failed to initialize app");

    // Initialize Cloud Storage and get a reference to the service
    let storage = get_storage_for_app(Some(app), None)
        .expect("storage component not available");

    let root = storage.root_reference().expect("missing bucket");
    println!("Root reference: {}", root.to_gs_url());
}
```



## Log from ChatGPT

### First run result

 I’m preparing a clear summary of the added Firebase Storage-like module in Rust, detailing file modifications with
  line references and mentioning new dependencies. I’ll note the cargo test execution with warnings from unrelated
  modules and suggest natural next steps like implementing network requests and emulator token integration.

> Added a first-pass Storage port that registers the service, parses bucket locations, and builds typed references in
  Rust.

  - Wired the storage component into the global registry and app containers so callers can resolve instances (including
  per-bucket overrides) via get_storage_for_app and emulator helpers (src/storage/api.rs:15, src/storage/mod.rs:10,
  src/lib.rs:6).
  - Implemented FirebaseStorageImpl with host/bucket state, emulator toggles, and reference construction while keeping
  auth/app-check providers ready for token work (src/storage/service.rs:15).
  - Ported location parsing, path utilities, and reference ergonomics with unit coverage to mirror the TypeScript
  behaviour for gs:// and HTTPS URLs (src/storage/location.rs:7, src/storage/path.rs:1, src/storage/reference.rs:6, src/
  storage/util.rs:1).
  - Added storage-specific error types and re-exported the storage surface; pulled in percent-encoding and url to
  support URL handling (src/storage/error.rs:4, Cargo.toml:15, Cargo.lock).

  Tests: cargo test (passes; still emits pre-existing dead-code warnings from other modules).
