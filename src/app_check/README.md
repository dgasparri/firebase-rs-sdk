# Firebase Firestore module

This module ports core pieces of the Firebase App Check SDK to Rust so applications can request, cache, and refresh attestation tokens that protect backend resources from abuse. It mirrors the JS SDK’s structure with an App Check façade, provider implementations, and internal wiring that other services (Firestore, Storage, etc.) can tap into via token providers.

It includes error handling, configuration options, and integration with Firebase apps.

## Features

- Initialize App Check for any FirebaseApp, choosing between the built-in reCAPTCHA providers or a custom provider callback.
- Toggle automatic token refresh and listen for token updates through observer APIs.
- Retrieve standard and limited-use App Check tokens on demand, receiving structured error details when attestation fails.
- Bridge App Check tokens into dependent services via FirebaseAppCheckInternal::token_provider so HTTP clients can attach X-Firebase-AppCheck headers automatically.
- Manage internal listeners (add/remove) and inspect cached token state for emulator or server-driven scenarios.

## Porting status

- app_check 20% `[##        ]`

==As of October 20th, 2025== 

Prompt: Compare the original JS/Typescript files in ./packages/app_check and the ported files in Rust in ./src/app_check, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust

I'm estimating the Rust implementation covers about 25-30% of the JavaScript App Check API, with many core parts missing like storage, proactive refresh, and debug support. I'm breaking down major areas to check coverage, especially around token handling, providers, and internal bridging, while planning to verify component registration details next.

Estimated coverage ≈20 % of the JS App Check SDK.

  - Implemented pieces cover only the happy-path API surface: initialize_app_check, token fetch, limited-use fetch,
  listener wiring, and manual auto-refresh toggling (src/app_check/api.rs:15, src/app_check/state.rs:37). Custom
  providers work for synchronous token callbacks (src/app_check/providers.rs:8), and there’s an internal bridge so other
  modules (e.g. Firestore) can treat App Check as a token provider (src/app_check/token_provider.rs:9, src/app_check/
  interop.rs:1).
  - All platform integrations that dominate the JS SDK are missing. There’s no component registration or service
  factory equivalent to the JS wiring in packages/app-check/src/factory.ts:33 and _registerComponent calls, so other
  modules cannot obtain App Check via the component system. Tokens only live in memory; the persistence layer, IndexedDB
  helpers, and debug-token storage (packages/app-check/src/storage.ts:32, packages/app-check/src/indexeddb.ts:1,
  packages/app-check/src/debug.ts:1) have no Rust counterpart.
  - Proactive refresh, throttling, and visibility-aware backoff are absent: the scheduler and retry policy in packages/
  app-check/src/proactive-refresh.ts:26 and the state-machine logic in packages/app-check/src/internal-api.ts:61 aren’t
  ported, leaving set_token_auto_refresh_enabled as a no-op flag (src/app_check/state.rs:98).
  - Provider support is skeletal. ReCAPTCHA providers immediately return “not implemented” errors (src/app_check/
  providers.rs:77, src/app_check/providers.rs:100), while the JS versions perform token acquisition, throttling, and
  heartbeat integration (packages/app-check/src/providers.ts:33). Debug-mode, emulator support, heartbeat usage, and
  observer error channels defined across packages/app-check/src/api.ts:34, packages/app-check/src/client.ts:1, and
  packages/app-check/src/util.ts:1 are missing entirely.

Given that only the minimal API shell and custom-provider plumbing exist while the JS module’s storage, refresh lifecycle, provider implementations, debug/emulator flows, and component factories are unported, a 20 % completion estimate is a reasonable upper bound.

## References to the Firebase JS SDK - firestore module

- QuickStart: <https://firebase.google.com/docs/app-check/web/recaptcha-provider>
- API: <https://firebase.google.com/docs/reference/js/app-check.md#app-check_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/app-check>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/app-check>

## Development status as of 14th October 2025

- Core functionalities: Some implemented (see the module's [README.md](https://github.com/dgasparri/firebase-rs-sdk/tree/main/src/app_check) for details)
- Tests: 4 tests (passed)
- Documentation: Some public functions are documented
- Examples: None provided

DISCLAIMER: This is not an official Firebase product, nor it is guaranteed that it has no bugs or that it will work as intended.

## Example Usage

```rust
use firebase_rs_sdk::app::*;
use firebase_rs_sdk::app_check::*;
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

1. **Async token fetchers**
   - Wire the newly introduced async token helpers (`get_token_async`, `get_limited_use_token_async`) to real network
     flows on both native and wasm targets, then update dependent services and documentation to consume them.
2. **Persistence layer**
   - Implement token storage abstraction and port the IndexedDB/localStorage fallbacks.
3. **Refresh scheduler**
   - Add proactive refresh logic with timer management, page visibility handling, and error retries.
4. **Recaptcha clients**
   - Complete reCAPTCHA provider implementations, including script injection, token exchange, and enterprise support.
5. **Debug token flow**
   - Mirror the JS debug token registration, persistence, and developer-mode warnings.
6. **Internal API parity**
   - Port limited-use token APIs, heartbeat wiring, and provider factory helpers from `internal-api.ts`/`factory.ts`.
7. **Testing parity**
   - Translate JS unit tests to Rust to cover refresh cycles, storage, and error paths.
8. **Documentation/examples**
   - Expand docs once the missing features land to show typical activation/refresh flows.

Addressing these items will bring the Rust App Check module up to feature parity with the JavaScript SDK and ready other
services to rely on its token guarantees.
