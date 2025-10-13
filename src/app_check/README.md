# Firebase App Check Port (Rust)

This directory tracks the Rust port of the Firebase App Check module. The intent is to replicate the behaviour of
`@firebase/app-check` so other services can obtain and refresh App Check tokens via the Rust component system.

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
