# Firebase Installations Module

The Installations module issues Firebase Installation IDs (FIDs) and scoped auth tokens used by other Firebase services. This Rust port mirrors the public JS API while speaking directly to the official Firebase Installations REST endpoints.

## Porting status
- installations 35% `[####      ]`

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
use firebase_rs_sdk::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::installations::{get_installations, InstallationToken};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
   let fid = installations.get_id().await?;
   let InstallationToken { token, expires_at } = installations.get_token(false).await?;
   println!("FID={fid}, token={token}, expires={expires_at:?}");
   Ok(())
}
```

## Implemented
- Component registration exposing `get_installations` with per-app caching (`src/installations/api.rs:146`).
- App config extraction and validation mirroring the JS helper (`src/installations/config.rs:6`).
- Async REST client with a native `reqwest` implementation and a WASM `fetch` backend behind the `wasm-web` feature (`src/installations/rest/native.rs:1`, `src/installations/rest/wasm.rs:1`).
- `Installations` public/internal APIs are now async, performing registration, token refresh, and delete operations without blocking (`src/installations/api.rs:112`).
- File-backed persistence for native targets and IndexedDB + BroadcastChannel-backed persistence for wasm builds (`src/installations/persistence.rs`).
- Internal helper that surfaces the full installation entry (FID, refresh token, auth token) for other modules such as Messaging (`src/installations/api.rs:185`).
- Unit tests covering config validation, async REST flows, persistence round-trips, delete behaviour, and service behaviour for forced refreshes (`src/installations/rest/tests.rs:1`, `src/installations/api.rs:472`, `src/installations/persistence.rs:80`).
- Private `installations-internal` component provides shared `get_id`/`get_token` helpers (`src/installations/api.rs:210`).

## Still to do
- Add concurrency coordination and migrations for the persistence layer (IndexedDB-style pending markers, multi-process guards).
- Implement JS parity APIs: `onIdChange` and internal factory helpers for other modules.
- Add ETag handling, heartbeat/X-Firebase-Client integration, and exponential backoff policies for REST requests.
- Provide emulator support, diagnostics logging, and richer error mapping from REST responses.
- Expand integration tests and shared fixtures to cover retry paths and error propagation.

## Next steps - Detailed completion plan
1. **Unblock Messaging’s FCM REST flow**
   - Expose a lightweight internal API that returns the installation entry (FID + refresh/auth tokens + expiry) so `src/messaging` can call the FCM registration endpoints.
   - Update messaging to replace the placeholder installation info with real data and add tests/docs covering the create/update/delete flows.
   - Add example snippets demonstrating how messaging can await the new async Installations APIs.
2. **Strengthen persistence coordination**
   - Mirror the JS pending-registration markers to avoid duplicate network calls when multiple awaiters race initialization.
   - Add retry/backoff policies on IndexedDB opening failures and surface structured telemetry for cache operations.
3. **Follow-on parity work**
   - Revisit JS parity items (`onIdChange`, heartbeat headers, emulator tooling) once the messaging integration settles.
   - Expand structured logging and diagnostics so native and wasm targets surface actionable errors to consuming modules.
