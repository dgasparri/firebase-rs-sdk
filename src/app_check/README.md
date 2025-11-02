# Firebase App Check module

## Introduction

This module ports Firebase App Check to Rust so client code can obtain, cache, and refresh attestation tokens that protect backend resources from abuse. The Rust port mirrors the modular JS SDK: it exposes an App Check façade, provider implementations, proactive refresh scheduling, IndexedDB persistence for wasm builds, and an internal bridge that other services (Firestore, Storage, etc.) can depend on.

## Porting status

- app_check 60% `[######    ]`

Significant parity milestones are now in place: App Check registers with the component system, background refresh follows the JS proactive-refresh heuristics (issued/expiry timestamps, jitter, exponential backoff), tokens persist across reloads on wasm targets, and storage, analytics, and other modules can request App Check tokens via the shared internal provider. ReCAPTCHA flows, debug tooling, and heartbeat integration remain unported, but the primary token lifecycle is functional and covered by tests.

## Quick Start Example

```rust,no_run
use firebase_rs_sdk::app::{initialize_app, FirebaseOptions};
use firebase_rs_sdk::app_check::{initialize_app_check, custom_provider, token_with_ttl, AppCheckOptions};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Initialise a Firebase app (see the `app` module for full options).
    let app = initialize_app(FirebaseOptions::default(), None).await.unwrap();

    // Register a simple custom App Check provider.
    let provider = custom_provider(|| token_with_ttl("dev-token", Duration::from_secs(300)));
    let options = AppCheckOptions::new(provider);
    let app_check = initialize_app_check(Some(app.clone()), options).await.unwrap();

    // Fetch and refresh tokens as needed.
    let token = app_check.get_token(false).await.expect("Could not get token");
    println!("token={}", token.token);

}
```

## References to the Firebase JS SDK - firestore module

- QuickStart: <https://firebase.google.com/docs/app-check/web/recaptcha-provider>
- API: <https://firebase.google.com/docs/reference/js/app-check.md#app-check_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/app-check>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/app-check>



## Implemented

- **Component registration & interop** (`api.rs`, `interop.rs`)
  - Public and internal App Check components register with the Firebase component system so other services can obtain tokens via `FirebaseAppCheckInternal`.
- **Token lifecycle management** (`state.rs`, `api.rs`)
  - In-memory cache with listener management, limited-use token support, and graceful error propagation when refreshes fail but cached tokens remain valid.
- **Proactive refresh scheduler** (`refresher.rs`, `api.rs`)
  - Matches the JS `proactive-refresh` policy (midpoint + 5 min offset, exponential backoff, cancellation) and automatically starts/stops based on auto-refresh settings.
- **Persistence & cross-tab broadcast** (`persistence.rs`)
  - IndexedDB persistence for wasm builds now records issued and expiry timestamps and broadcasts token updates across tabs; native builds fall back to memory cache.
- **Heartbeat telemetry** (`types.rs`, `api.rs`)
  - Integrates with the shared heartbeat service so outgoing requests can attach the `X-Firebase-Client` header (Storage, Firestore, Functions, Database, and AI clients now call through `FirebaseAppCheckInternal::heartbeat_header`).
- **Tests & tooling** (`api.rs`, `interop.rs`, `token_provider.rs`, `storage/service.rs`)
  - Unit tests cover background refresh, cached-token error handling, internal listener wiring, and Storage integration; shared test helpers ensure state isolation.

## Still to do

- Full reCAPTCHA v3/Enterprise client implementations (script bootstrap, throttling, heartbeat awareness).
- Debug token developer mode, emulator toggles, and console logging parity.
- Web-specific visibility listeners and throttling heuristics (document visibility, pause on hidden tabs).
- Broader provider catalogue (App Attest, SafetyNet) and wasm-friendly abstractions for platform bridges.

## Next steps – Detailed completion plan

1. **Implement reCAPTCHA providers**
   - Port `client.ts`/`recaptcha.ts`, including script injection, widget lifecycle, and throttling metadata; surface provider configuration errors through the Rust error enum.
2. **Debug/emulator workflow**
   - Persist debug tokens, expose APIs to toggle debug mode, and surface console hints mirroring `debug.ts`; ensure emulator host/port wiring is available to downstream services.
3. **Internal API parity**
   - Port the remaining `internal-api.ts` helpers (limited-use exchange wrappers, throttling metadata) so downstream services can rely on the same behaviours as the JS SDK.
4. **Visibility-aware refresh controls**
   - Add document visibility listeners on wasm targets and equivalent hooks for native platforms so refresh pauses/resumes follow the JS scheduler behaviour.
5. **Expand tests & docs**
   - Backfill the JS unit scenarios (refresh retry tables, storage integration failures) and extend rustdoc/README guidance, including wasm-specific notes and provider examples.
