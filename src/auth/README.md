# Firebase Auth Port (Rust)

This directory contains the Rust port-in-progress of the Firebase Auth SDK. The aim is to reproduce the modular
`@firebase/auth` surface while reusing shared app/component infrastructure.

## What’s Implemented

- **Auth service** (`api.rs`, `mod.rs`)
  - Component registration and `Auth` factory so apps can resolve an `Auth` instance via `get_provider("auth")`.
  - `Auth::builder` helper that lets consumers inject custom persistence implementations before initialization.
- **OAuth scaffolding** (`oauth/`)
  - Defines `OAuthRequest`, popup/redirect handler traits, and the new `OAuthProvider` builder (`sign_in_with_popup` /
    `sign_in_with_redirect` / `get_redirect_result`). Apps register platform-specific implementations via
    `Auth::builder().with_popup_handler(...)` / `.with_redirect_handler(...)`, keeping UI and JS glue outside the core crate.
  - Reference adapters live in `examples/oauth_popup_wasm.rs` (WASM + JS) and `examples/oauth_redirect_desktop.rs`
    (system browser) to demonstrate handler wiring end to end.
  - Provider convenience builders (`GoogleAuthProvider`, `FacebookAuthProvider`, etc.) and redirect persistence helpers
    mirror the JS SDK strategies while remaining platform agnostic.
  - `Auth::builder().with_redirect_persistence(...)` allows apps to supply durable storage for pending redirect events.
- **REST client scaffolding** (`api.rs`)
  - Minimal email/password endpoint support (`signInWithPassword`, `signUp`) with token storage.
  - `signInWithIdp` REST exchange for OAuth credentials produced by popup/redirect handlers.
- **Account management APIs** (`api/account.rs`)
  - Password reset, email verification, profile/email/password updates, user deletion, and provider linking are wired via
    the Firebase Auth REST endpoints, surfaced through new `Auth` helpers. Re-authentication helpers cover both password
    and OAuth credentials.
- **Models & types** (`model.rs`, `types.rs`)
  - User models (`User`, `UserCredential`), provider structs (`EmailAuthProvider`), action code types, token metadata.
- **Errors & result handling** (`error.rs`)
  - Auth-specific error enumeration and result aliases.
- **Module wiring** (`mod.rs`)
  - Public re-exports and component registration entrypoints used by other services.
- **Token refresh scaffolding** (`token_manager.rs`, `api/token.rs`, `token_provider.rs`)
  - Tracks ID/refresh tokens with expiry metadata, hits the Secure Token endpoint on demand, and exposes an
    `AuthTokenProvider` that other services (Firestore, Functions) can plug in for authenticated requests.
- **State persistence (in-memory + web storage)** (`persistence/`)
  - Saves/restores the signed-in user and tokens across restarts via a pluggable persistence interface. Native builds
    default to an in-memory driver, while WASM targets can enable the `wasm-web` feature to use local/session storage
    with multi-tab coordination through the new `WebStoragePersistence` adapter.

This functionality allows email/password sign-in and basic user state handling, enabling dependent modules to retrieve
`Auth` instances.

## Gaps vs `packages/auth`

The JavaScript implementation is significantly broader. Missing pieces include:

1. **Multi-factor authentication (MFA)**
   - `mfa/` flows, second-factor enrollment, and related error handling are absent.
2. **Credential providers**
   - OAuth providers (Google, Facebook, GitHub, etc.), phone auth, and custom token exchange are not ported.
3. **Persistence & storage**
   - IndexedDB persistence and native (non-web) durable adapters are still missing. Web builds rely on `WebStoragePersistence`
     (local/session storage) when the `wasm-web` feature is enabled; additional backends and fallbacks are needed for full parity.
   - Consumers can wire their own persistence via `Auth::builder().with_persistence(...)` and the `ClosurePersistence`
     helper when the platform requires bespoke storage.
4. **Token refresh & proactive refresh**
   - Advanced refresh orchestration, emulator hooks, and `beforeAuthStateChanged` callbacks are still outstanding.
5. **Platform-specific implementations**
   - Cordova, React Native, and browser-specific glue (popup/redirect flows, OAuth helpers) are not implemented.
6. **Linking/account management**
   - APIs for linking providers, re-authenticating, password reset, email verification, etc.
7. **Advanced REST endpoints**
   - Secure token exchange, custom claims, STS, and emulator support require additional REST integrations.
8. **Testing parity**
   - Extensive JS unit/integration tests (core, API, provider flows) have not been translated.

## Next Steps

1. **Persistence layer**
   - Add IndexedDB persistence, storage selection policies, and native (desktop/server) durability options to complement
     the existing in-memory and web storage adapters.
2. **Token lifecycle**
   - Flesh out refresh observers (`beforeAuthStateChanged`), emulator behaviour, and more granular backoff/queueing.
3. **Provider expansion**
   - Port OAuth provider infrastructure (credential building, popup/redirect flows) and phone auth scaffolding.
4. **Account management APIs**
   - Add action code handling (email verification completion, password reset lookup) and richer error mapping to
     complement the password reset / verification / update / delete / re-auth flows now available.
5. **MFA**
   - Bring over the MFA subsystem (enrollment, challenge, resolver objects).
6. **Platform adapters**
   - Add browser/React Native/Cordova specific implementations for popup/redirect handlers and persistence quirks.
   - Provide reference crates or documentation for hooking the new OAuth handler traits into common environments (WASM,
     desktop webviews, native mobile shells).
   - Ship example adapters exercising the handler traits end-to-end (WASM popup, desktop redirect) to validate the API
     surface and ease consumer adoption.
7. **Testing**
   - Translate core JS tests to cover sign-in flows, persistence, provider behaviour, and token refresh.

## Immediate Porting Focus (authenticated consumers)

| Priority | Status | JS source | Target Rust module | Scope | Key dependencies |
|----------|--------|-----------|--------------------|-------|------------------|
| P0 | done | `packages/auth/src/core/user/token_manager.ts`, `core/user/id_token_listener.ts` | `src/auth/token_manager.rs`, `model.rs` | Port the STS token manager: track expiry, fire token-change listeners, and queue proactive refresh hooks. | Reuse `util::backoff`, add wall-clock helpers, persist expiry metadata. |
| P0 | done | `packages/auth/src/api/authentication/token.ts`, `api/helpers.ts` | `src/auth/api/token.rs` | Implement secure token exchange (`grant_type=refresh_token`) so refresh tokens issue new ID/access tokens. | Shares REST client, needs detailed error mapping to `AuthErrorCode`. |
| P0 | done | `packages/auth/src/core/auth/auth_impl.ts`, `core/credentials/auth_token_provider.ts` | `src/auth/token_provider.rs`, integrate with `firestore::remote::datastore::TokenProvider` | Expose an `AuthTokenProvider` that watches `Auth` state and hands out fresh tokens to dependent services (Firestore, Functions, etc.). | Depends on token manager + refresh API; requires component registration for cross-service injection. |
| P0 | done | `packages/auth/src/core/persistence/**/*` | `src/auth/persistence/` | Base in-memory persistence with hydration plus WASM web storage adapters and multi-tab sync; extend to IndexedDB/native drivers next. | Needs storage abstraction; coordinate with app/platform modules. |
| P1 | done | `packages/auth/src/api/account_management/*.ts`, `core/user/user_impl.ts` | `src/auth/api/account.rs` | Fill in user management flows (password reset, email verification, updates, delete, linking). | Builds on authenticated REST client and persistence. |
| P1 | `packages/auth/src/core/strategies/*`, `api/idp/*.ts` | `src/auth/oauth/` | Port OAuth provider scaffolding to broaden sign-in options beyond email/password. | Requires platform adapters for popup/redirect flows. |

Delivering the P0 items unlocks authenticated Firestore/Functions calls by providing an auto-refreshing token pipeline while laying the groundwork for broader auth parity.

Completing these steps will move the Rust Auth module closer to feature parity with the JavaScript SDK and make it viable
for a broader set of authentication scenarios.

## Test Porting Roadmap

This section tracks the JavaScript test surface in `packages/auth` and maps it to Rust parity work in `src/auth`. Use it as the master checklist when translating or replacing test suites.

### Inventory by Area
- **Core & API** – `src/api/authentication/*.test.ts`, `src/api/account_management/*.test.ts`, `src/api/index.test.ts`, password-policy and project-config tests, plus `src/core/auth/*.test.ts`, `src/core/strategies/*.test.ts`, and `src/core/util/*.test.ts`.
- **User, Providers, Persistence, MFA** – `src/core/user/*.test.ts`, `src/core/providers/*.test.ts`, `src/core/credentials/*.test.ts`, `src/core/persistence/*.test.ts`, `src/mfa/*.test.ts`, and `src/mfa/assertions/totp.test.ts`.
- **Platform Browser & React Native** – Browser auth, popup/redirect, strategies (`phone`, `popup`, `redirect`), recaptcha suites, message channel, iframe, and persistence tests (`browser`, `indexed_db`, `local_storage`, `session_storage`), plus React Native persistence.
- **Cordova** – Popup redirect `events`, `popup_redirect`, and `utils` tests under `src/platform_cordova/popup_redirect`.
- **Integration & Harness** – Webdriver suites (`anonymous`, `persistence`, `popup`, `redirect`, `compat/firebaseui`), flow tests (`anonymous`, `custom.local`, `email`, `firebaseserverapp`, `hosting_link`, `idp.local`, `oob.local`, `password_policy`, `phone`, `recaptcha_enterprise`, `totp`), helpers, and `scripts/run_node_tests.ts`.

### Porting Strategy
- **Gap Analysis** – For every Rust module, highlight missing unit/integration coverage in `README.md` issue tracker and open follow-up tickets when core functionality is absent (tests should never outrun implementation).
- **Rust Unit Tests First** – Embed `#[cfg(test)]` modules alongside Rust source for pure logic (core util, strategies, models) and mirror JS describe blocks with idiomatic Rust test functions.
- **Mocked REST Validation** – Replace JS REST mocks with Rust HTTP stubs (`httpmock`, `wiremock`) to assert request payloads, error mapping, and token handling for authentication/account endpoints.
- **Persistence & Token Lifecycle** – Port storage and token manager tests using in-memory fakes; add `wasm-bindgen-test` variants behind the `wasm-web` feature to cover local/session storage semantics and multi-tab coordination.
- **Provider & MFA Suites** – Translate credential/provider/MFA tests incrementally as implementations land. Add feature-gated WASM tests for popup/redirect contracts, and define trait-based shims to represent unimplemented platform adapters.
- **Integration Replacement** – Recreate end-to-end flows within Rust integration tests (`tests/` directory) using mocked transport layers. Reserve browser automation for consumer projects; expose hook traits so external harnesses can drive real WebDriver flows.
- **Tooling & Execution** – Consolidate around `cargo test` targets. Introduce scenario-specific test harness crates when orchestration is needed and document feature flags/environment expectations here after each milestone.

### Recent Progress
- Added Rust unit coverage for email/password sign-in and account creation flows (`src/auth/api.rs`) using `httpmock` to emulate Identity Toolkit responses.
- Exercised secure token exchange success and error paths through `refresh_id_token_with_endpoint` tests (`src/auth/api/token.rs`).
- Introduced shared mock/test helpers (`src/test_support/`) so additional modules can reuse Firebase app and HTTP server scaffolding.

Keep this roadmap updated as suites are ported: mark completed migrations, link Rust test modules, and note any design deviations from the JavaScript originals.
