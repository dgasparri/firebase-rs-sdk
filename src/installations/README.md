# Firebase Installations Module

The Installations module issues Firebase Installation IDs (FIDs) and scoped auth tokens used by other Firebase services. This Rust port mirrors the public JS API while speaking directly to the official Firebase Installations REST endpoints.

## Porting status
- installations 35% [####      ]

==As of October 20th, 2025==

Prompt: Compare the original JS/Typescript files in ./packages/installations and the ported files in Rust in ./src/installations, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust

I’m reviewing the Rust and JS implementations to compare their feature coverage. The Rust code now performs real Installation registration and token refresh via the REST API, replacing the previous stub. Persistence, concurrency coordination, and secondary APIs (delete, onIdChange) still need to be ported. Given the new network integration, coverage increases from ~5 % to roughly 15 %. I’ll reference specific file paths and lines to support this analysis and highlight the biggest gaps before crafting the final summary.

Coverage is around 35 %.

  - Network registration and token refresh mirror the JS helpers (`packages/installations/src/functions/create-installation-request.ts`, `generate-auth-token-request.ts`) through a blocking `reqwest` client (`src/installations/rest.rs:1`).
  - `get_id` and `get_token` call through the REST client and cache the resulting FID/token pair like the JS `getInstallationEntry` flow (`src/installations/api.rs:23`).
  - Configuration extraction (`src/installations/config.rs:6`) validates required app options, similar to JS `extractAppConfig`.
  - Installation entries (FID + refresh/auth tokens) persist to disk between runs using the default file-backed cache (`src/installations/persistence.rs:1`).
  - `delete_installations` mirrors the JS delete flow, issuing the REST delete call and clearing cached state (`src/installations/api.rs:200`).
  - onIdChange callbacks, IndexedDB-style concurrency guards, richer retry/backoff policies, and emulator/diagnostics features remain outstanding.

## Quick Start Example
```rust,no_run
use firebase_rs_sdk_unofficial::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk_unofficial::installations::{get_installations, InstallationToken};

fn main() -> Result<(), Box<dyn std::error::Error>> {
   let app = initialize_app(
       FirebaseOptions {
           api_key: Some("AIza...".into()),
           project_id: Some("my-project".into()),
           app_id: Some("1:123:web:abc".into()),
           ..Default::default()
       },
       Some(FirebaseAppSettings::default()),
   )?;

   let installations = get_installations(Some(app.clone()))?;
   let fid = installations.get_id()?;
   let InstallationToken { token, expires_at } = installations.get_token(false)?;
   println!("FID={fid}, token={token}, expires={expires_at:?}");
   Ok(())
}
```

## Implemented
- Component registration exposing `get_installations` with per-app caching (`src/installations/api.rs:146`).
- App config extraction and validation mirroring the JS helper (`src/installations/config.rs:6`).
- REST client that registers installations and generates auth tokens using `reqwest` with retry on server errors (`src/installations/rest.rs:15`).
- `get_id` now performs real registration, caches the returned FID/refresh token/auth token trio, and persists it to disk (`src/installations/api.rs:63`).
- `get_token(force_refresh)` refreshes expired tokens via the REST endpoint, updates the cached entry, and writes the refreshed token back to the persistence store (`src/installations/api.rs:76`).
- `delete_installations` removes the registered installation via REST, clears in-memory state, and deletes the persisted cache entry (`src/installations/api.rs:200`).
- File-backed persistence that stores a per-app JSON record and loads it on startup (`src/installations/persistence.rs:1`).
- Unit tests covering config validation, REST serialization/parsing (skipping when sockets are unavailable), persistence round-trips, delete behaviour, and service behaviour for forced refreshes (`src/installations/rest.rs:156`, `src/installations/api.rs:188`, `src/installations/persistence.rs:80`).
- Private `installations-internal` component provides shared `get_id`/`get_token` helpers (`src/installations/api.rs:210`).

## Still to do
- Add concurrency coordination and migrations for the persistence layer (IndexedDB-style pending markers, multi-process guards).
- Implement JS parity APIs: `onIdChange` and internal factory helpers for other modules.
- Add ETag handling, heartbeat/X-Firebase-Client integration, and exponential backoff policies for REST requests.
- Support multi-environment behaviour (web/WASM vs native) including pluggable storage backends.
- Provide emulator support, diagnostics logging, and richer error mapping from REST responses.
- Expand integration tests and shared fixtures to cover retry paths and error propagation.

## Next steps - Detailed completion plan
1. **Harden persistence coordination**
   - Mirror JS pending-registration semantics by tracking registration status in the persisted entry and short-circuiting duplicate create requests.
   - Add a lightweight file lock or cross-process signal to prevent concurrent writes from clobbering data.
   - Expose a pluggable trait implementation for WASM (IndexedDB) and native (filesystem/DB) backends.
2. **Port lifecycle APIs**
   - Expose `on_id_change` callbacks with deduplication similar to `generateFidChanged` in JS, making sure the notification fires after persistence updates.
   - Add internal factory helpers exposing ID/token operations to dependent services.
3. **Augment REST client robustness**
   - Add ETag management, exponential backoff, and richer error codes derived from server responses.
   - Integrate heartbeat header support behind the `wasm-web` feature once a heartbeat provider exists in `FirebaseApp`.
