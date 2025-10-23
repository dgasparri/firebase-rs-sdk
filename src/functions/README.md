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
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    register_functions_component();

    let app = initialize_app(
        FirebaseOptions {
            project_id: Some("demo-project".into()),
            ..Default::default()
        },
        Some(FirebaseAppSettings::default()),
    )?;

    let functions = get_functions(Some(app), None)?;
    let callable = functions
        .https_callable::<serde_json::Value, serde_json::Value>("helloWorld")?;

    let response = callable.call(&json!({ "message": "hi" }))?;
    println!("response: {response:?}");
    Ok(())
}
```

## Implemented

- Component registration so `Functions` instances can be resolved from a `FirebaseApp` container.
- Native HTTPS callable transport built on `reqwest::blocking`, returning decoded JSON payloads.
- Error code mapping aligned with the JS SDK (`packages/functions/src/error.ts`) including backend
  `status` translation and message propagation.
- Custom-domain targeting (including emulator-style origins) by interpreting the instance
  identifier passed to `get_functions`.
- Unit test (`https_callable_invokes_backend`) that validates request/response wiring against an
  HTTP mock server (skips automatically when sockets are unavailable).
- Context provider that pulls Auth and App Check credentials from the component system and injects
  the appropriate `Authorization` / `X-Firebase-AppCheck` headers on outgoing callable requests.

## Still to do

- Messaging token lookup so callable requests can include the
  `Firebase-Instance-ID-Token` header when FCM is configured (`packages/functions/src/context.ts`).
- Serializer parity for Firestore Timestamp, GeoPoint, and other special values used by callable
  payloads (`packages/functions/src/serializer.ts`).
- Emulator helpers (`connectFunctionsEmulator`) and environment detection to configure the base
  origin (`packages/functions/src/service.ts`).
- Streaming callable support (`httpsCallable().stream`) that handles server-sent events.
- Public helpers like `httpsCallableFromURL` and region selection utilities from
  `packages/functions/src/api.ts`.
- Comprehensive error detail decoding (custom `details` payload) and cancellation handling.
- Broader test coverage mirroring `packages/functions/src/callable.test.ts`.

## Next steps - Detailed completion plan

1. **Cross-platform callable transport**
   - Define a `CallableTransport` trait in `src/functions/transport.rs` with both native and wasm
     implementations.
   - Keep the current blocking reqwest path under `cfg(not(target_arch = "wasm32"))` and add a stub
     wasm transport that will later be backed by `web_sys::fetch`.
   - Adjust `CallableFunction::call` to delegate through the trait so transports can be swapped
     without touching higher layers.
2. **Async context plumbing**
   - Make the functions context provider async to mirror the JS `ContextProvider`, then surface
     asynchronous callable APIs (`call_async`) while keeping blocking helpers for native targets.
   - Ensure auth/app-check/messaging token fetchers expose async entry points on wasm and native
     (wrapping in `block_on` where needed for compatibility).
3. **Messaging token integration**
   - Expose a cached FCM token accessor from the messaging module so the callable context can attach
     the `Firebase-Instance-ID-Token` header when available.
4. **Serializer parity & URL utilities**
   - Port the callable serializer helpers (Firestore timestamps, GeoPoints) and add helpers such as
     `https_callable_from_url` and `connect_functions_emulator` for full API coverage.
5. **Testing & documentation**
   - Add wasm-specific integration tests using `wasm-bindgen-test`, update examples, and expand the
     README with wasm configuration notes once the transports and async APIs land.
