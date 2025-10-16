# Firebase Firestore module

This module ports core pieces of the Firebase App Check SDK to Rust so applications can request, cache, and refresh attestation tokens that protect backend resources from abuse. It mirrors the JS SDK’s structure with an App Check façade, provider implementations, and internal wiring that other services (Firestore, Storage, etc.) can tap into via token providers.

It includes error handling, configuration options, and integration with Firebase apps.

## Features

- Initialize App Check for any FirebaseApp, choosing between the built-in reCAPTCHA providers or a custom provider callback.
- Toggle automatic token refresh and listen for token updates through observer APIs.
- Retrieve standard and limited-use App Check tokens on demand, receiving structured error details when attestation fails.
- Bridge App Check tokens into dependent services via FirebaseAppCheckInternal::token_provider so HTTP clients can attach X-Firebase-AppCheck headers automatically.
- Manage internal listeners (add/remove) and inspect cached token state for emulator or server-driven scenarios.

## References to the Firebase JS SDK - firestore module

- QuickStart: <https://firebase.google.com/docs/app-check/web/recaptcha-provider>
- API: <https://firebase.google.com/docs/reference/js/app-check.md#app-check_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/app-check>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/app-check>

## Development status as of 14th October 2025

- Core functionalities: Some implemented (see the module's [README.md](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/app_check) for details)
- Tests: 4 tests (passed)
- Documentation: Some public functions are documented
- Examples: None provided

DISCLAIMER: This is not an official Firebase product, nor it is guaranteed that it has no bugs or that it will work as intended.

## Example Usage

```rust
use firebase_rs_sdk_unofficial::app::*;
use firebase_rs_sdk_unofficial::app_check::*;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    // Configure the Firebase project. Replace these placeholder values with your
    // own Firebase configuration when running the sample against real services.
    let options = FirebaseOptions {
        api_key: Some("YOUR_WEB_API_KEY".into()),
        project_id: Some("your-project-id".into()),
        app_id: Some("1:1234567890:web:abcdef".into()),
        ..Default::default()
    };

    let settings = FirebaseAppSettings {
        name: Some("app-check-demo".into()),
        automatic_data_collection_enabled: Some(true),
    };

    let app = initialize_app(options, Some(settings))?;

    // Create a simple provider that always returns the same demo token.
    let provider = custom_provider(|| token_with_ttl("demo-app-check", Duration::from_secs(60)));
    let options = AppCheckOptions::new(provider.clone()).with_auto_refresh(true);

    let app_check = initialize_app_check(Some(app.clone()), options)?;

    // Enable or disable automatic background refresh.
    set_token_auto_refresh_enabled(&app_check, true);

    // Listen for token updates. The listener fires immediately with the cached token
    // (if any) and then on subsequent refreshes.
    let listener: AppCheckTokenListener = Arc::new(|result| {
        if !result.token.is_empty() {
            println!("Received App Check token: {}", result.token);
        }
        if let Some(error) = &result.error {
            eprintln!("App Check token error: {error}");
        }
    });
    let handle = add_token_listener(&app_check, listener, ListenerType::External)?;

    // Retrieve the current token and a limited-use token.
    let token = get_token(&app_check, false)?;
    println!("Immediate token fetch: {}", token.token);

    let limited = get_limited_use_token(&app_check)?;
    println!("Limited-use token: {}", limited.token);

    // Listener handles implement Drop and automatically unsubscribe, but you can
    // explicitly disconnect if desired.
    handle.unsubscribe();

    delete_app(&app)?;
    Ok(())
}
```

## What’s Implemented

- **Public API surface** (`api.rs`)
  - Registration of the App Check component, `initialize_app_check`, `get_token`, listener management, and
    emulator toggles.
- **Token state management** (`state.rs`)
  - In-memory token cache with listener registration/unregistration and auto-refresh flags.
- **Providers** (`providers.rs`)
  - Debug, reCAPTCHA, and limited-use provider scaffolding (mirroring factory wiring in JS) with placeholder behaviour.
- **Interop surface** (`interop.rs`)
  - Internal interfaces (`FirebaseAppCheckInternal`) to integrate with other services that depend on the internal
    provider.
- **Types & errors** (`types.rs`, `errors.rs`)
  - Ported core enums, token/result structs, and error variants.
- **Logging** (`logger.rs`)
  - Simple logger wrapping the shared `app::LOGGER` for App Check–specific messages.

This functionality allows registration of App Check providers, manual token fetching, and listener callbacks in line
with the JS modular API, albeit without full refresh scheduling or storage guarantees.

## Gaps vs `packages/app-check`

Comparing to the TypeScript implementation reveals functionality that still needs to be ported:

1. **Token storage & persistence**
   - IndexedDB/localStorage-backed persistence (`indexeddb.ts`, `storage.ts`) is not implemented; tokens live only in
     memory.
2. **Proactive refresh**
  - Refresh scheduler (`proactive-refresh.ts`) with exponential backoff and visibility listeners is missing.
3. **Recaptcha flows**
   - Full reCAPTCHA/enterprise client logic (`client.ts`, `recaptcha.ts`, widget management) is stubbed in the Rust
     providers.
4. **Debug token support**
   - Debug token handling and storage is only partially mirrored (`debug.ts` behaviour not fully ported).
5. **Internal API features**
   - JS `internal-api.ts` includes bridge functions for limited-use tokens and heartbeat integration not yet ported.
6. **Utility helpers**
   - `util.ts` (e.g., caching helpers, canonicalization) and measurement logging are outstanding.
7. **Tests**
   - Port of the unit test suite covering token lifecycle, provider factories, and storage interactions is pending.

## Next Steps

1. **Persistence layer**
   - Implement token storage abstraction and port the IndexedDB/localStorage fallbacks.
2. **Refresh scheduler**
   - Add proactive refresh logic with timer management, page visibility handling, and error retries.
3. **Recaptcha clients**
   - Complete reCAPTCHA provider implementations, including script injection, token exchange, and enterprise support.
4. **Debug token flow**
   - Mirror the JS debug token registration, persistence, and developer-mode warnings.
5. **Internal API parity**
   - Port limited-use token APIs, heartbeat wiring, and provider factory helpers from `internal-api.ts`/`factory.ts`.
6. **Testing parity**
   - Translate JS unit tests to Rust to cover refresh cycles, storage, and error paths.
7. **Documentation/examples**
   - Expand docs once the missing features land to show typical activation/refresh flows.

Addressing these items will bring the Rust App Check module up to feature parity with the JavaScript SDK and ready other
services to rely on its token guarantees.
