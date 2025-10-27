# Firebase Functions Port (Rust)

## Introduction

The `functions` module provides a Rust port of the Firebase Cloud Functions (client) SDK so
applications can invoke HTTPS callable backends from native or WASM targets. The goal is to mirror
the modular JavaScript API (`@firebase/functions`) while using idiomatic Rust primitives.

## Porting status

- functions 25% `[###       ]`

(Estimated after landing the callable transport, context headers, and persistence plumbing on
October 20th, 2025.)

## Quick Start Example

```rust,no_run
use firebase_rs_sdk::app::api::initialize_app;
use firebase_rs_sdk::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::functions::{get_functions, register_functions_component};
use serde_json::{json, Value as JsonValue};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    register_functions_component();

    let app = initialize_app(
        FirebaseOptions {
            project_id: Some("demo-project".into()),
            ..Default::default()
        },
        Some(FirebaseAppSettings::default()),
    )
    .await?;

    let functions = get_functions(Some(app.clone()), None).await?;
    let callable = functions
        .https_callable::<JsonValue, JsonValue>("helloWorld")?;

    let response = callable
        .call_async(&json!({ "message": "hi" }))
        .await?;
    println!("response: {response:?}");
    Ok(())
}
```

Use your platform's async runtime to drive the `main` future (for example `#[tokio::main]` on
native targets or `wasm_bindgen_futures::spawn_local` on `wasm32-unknown-unknown`).

## Implemented

- Component registration so `Functions` instances can be resolved from a `FirebaseApp` container.
- Native HTTPS callable transport backed by async `reqwest::Client`, exposed through an async
  transport layer.
- Public callable API is async-only so the module works seamlessly on native and WASM targets.
- Error code mapping aligned with the JS SDK (`packages/functions/src/error.ts`) including backend
  `status` translation and message propagation.
- Custom-domain targeting (including emulator-style origins) by interpreting the instance
  identifier passed to `get_functions`.
- Unit test (`https_callable_invokes_backend`) that validates request/response wiring against an
  HTTP mock server (skips automatically when sockets are unavailable).
- Context provider that pulls Auth and App Check credentials from the component system and injects
  the appropriate `Authorization` / `X-Firebase-AppCheck` headers on outgoing callable requests.
- WASM transport that issues callable requests through `fetch`, mirrors native timeout semantics
  with `AbortController`, and reuses shared error mapping.
- Automatic FCM token lookup so callable requests include the
  `Firebase-Instance-ID-Token` header when the Messaging component is configured and a cached (or
  freshly resolved) token is available.

## Still to do

- Serializer parity for Firestore Timestamp, GeoPoint, and other special values used by callable
  payloads (`packages/functions/src/serializer.ts`).
- Emulator helpers (`connectFunctionsEmulator`) and environment detection to configure the base
  origin (`packages/functions/src/service.ts`).
- Streaming callable support (`httpsCallable().stream`) that handles server-sent events.
- Public helpers like `httpsCallableFromURL` and region selection utilities from
  `packages/functions/src/api.ts`.
- Comprehensive error detail decoding (custom `details` payload) and cancellation handling.
- Broader test coverage mirroring `packages/functions/src/callable.test.ts`.
- Restore Firebase Messaging token forwarding on WASM once the messaging module exposes a wasm-web
  implementation.

## Next steps - Detailed completion plan

1. **Serializer parity & URL utilities**
   - Port the callable serializer helpers (Firestore timestamps, GeoPoints) and add helpers such as
     `https_callable_from_url` and `connect_functions_emulator` for full API coverage.
2. **Wasm validation & tooling**
   - Stand up `wasm-bindgen-test` coverage to verify the fetch transport, timeout handling, and
     header propagation in a browser-like environment.
   - Document the crate features (`wasm-web`) and any required Service Worker setup in the README
     example section.
3. **Advanced error handling**
   - Surface backend `details` payloads and cancellation hooks so callers can differentiate retryable
     vs. fatal failures, matching `packages/functions/src/error.ts` semantics.
4. **Async adoption sweep**
   - Audit the workspace for callers of `get_functions` and `CallableFunction::call_async`, updating
     each site to await the async APIs and adjusting examples/docs where necessary.
   - Re-run the smoke scripts (`scripts/smoke.sh` / `.bat`) to confirm both native and wasm builds
     stay green after the sweep.
