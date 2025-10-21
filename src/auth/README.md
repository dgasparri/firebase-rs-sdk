# Firebase Authentication module

This module ports core pieces of the Firebase Authentication SDK to Rust.

It provides functionality to interact with Firebase Authentication, allowing users to sign in to your app using one or more sign-in methods, including email address and password sign-in, and federated identity providers such as Google Sign-in and Facebook Login. Through the SDK it is also possible to manage the user's credentials.

It includes error handling, configuration options, and integration with Firebase apps.

## Features

- Initialize Auth for any FirebaseApp, with optional custom persistence.
- Sign users in/out via email + password, IDP (OAuth) flows, and custom tokens once implemented.
- Observe auth state changes and access the current User snapshot.
- Refresh and manage ID tokens, exposing them through the AuthTokenProvider bridge for other services.
- Access provider helpers (e.g., EmailAuthProvider) and user profile updates, matching the JS API ergonomics.

## Porting status

- auth 25% \[###       \]

==As of October 21th, 2025== 

Prompt: Compare the original JS/Typescript files in ./packages/auth and the ported files in Rust in ./src/auth, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust for this module

Porting Estimate

  - Roughly ~25% parity: Rust covers core email/password REST flows and some OAuth scaffolding, but large swaths of
  the JS module (custom token, phone, MFA, browser helpers, advanced settings) remain unported (packages/auth/src/api/
  index.ts:56, packages/auth/src/core/index.ts:292).

Coverage Highlights

  - Email/password sign-in, account creation, and reauthentication are implemented (src/auth/api.rs:147, src/auth/
  api.rs:184, src/auth/api.rs:533), aligning with the JS email/password strategy basics.
  - Account management helpers—password reset, email verification, profile/email/password updates, deletion, and
  unlinking—are present (src/auth/api.rs:429, src/auth/api.rs:458, src/auth/api.rs:503, src/auth/api.rs:515).
  - OAuth credential exchange plus popup/redirect abstractions exist, letting consumers plug platform handlers while
  reusing signInWithIdp (src/auth/api.rs:421, src/auth/oauth/provider.rs:1, src/auth/oauth/redirect.rs:1).
  - Persistence and token refresh infrastructure (in-memory/closure storage and Secure Token client) mirror the JS
  concepts at a basic level (src/auth/persistence/mod.rs:1, src/auth/api/token.rs:1), with a wasm-web storage variant
  (src/auth/persistence/web.rs:1).

Major Gaps

  - JS exposes many additional endpoints—custom token, email link, phone SMS, reCAPTCHA config, password policy, token
  revocation—that the Rust side lacks (packages/auth/src/api/index.ts:56).
  - Strategy helpers for anonymous sign-in, email link flows, credential-based link/reauth, and verification utilities
  remain unported (packages/auth/src/core/index.ts:292).
  - Multi-factor enrollment/resolution, MFA-specific errors, and assertions are fully implemented in JS (packages/auth/
  src/mfa/mfa_resolver.ts:37) but reduced to stubs returning NotImplemented in Rust (src/auth/types.rs:104).
  - Browser-specific pieces—popup/redirect resolvers, iframe messaging, reCAPTCHA bootstrap, indexedDB/local/session
  storage adapters—are absent on the Rust side (packages/auth/src/platform_browser/popup_redirect.ts:1, packages/auth/
  src/platform_browser/load_js.ts:1).
  - Auth settings, localization, tenant management, onIdTokenChanged listeners, emulator support, and rich error/code
  mapping exported in JS (packages/auth/src/core/index.ts:204) have no Rust equivalents yet.

Next Steps

  1. Prioritize porting additional REST endpoints (custom token, phone, email link, token revoke) to close the biggest
  functional gaps.
  2. Introduce multi-factor primitives and richer listener/error plumbing to match the JS surface before tackling
  browser-specific adapters.



## References to the Firebase JS SDK - firestore module

- QuickStart: <https://firebase.google.com/docs/auth/web/start>
- API: <https://firebase.google.com/docs/reference/js/auth.md#auth_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/auth>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/auth>

## Development status as of 14th October 2025

- Core functionalities: Mostly implemented (see the module's [README.md](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/auth) for details)
- Tests: 30 tests (passed)
- Documentation: Most public functions are documented
- Examples: 1 provided

DISCLAIMER: This is not an official Firebase product, nor it is guaranteed that it has no bugs or that it will work as intended.

## Example Usage

```rust
use firebase_rs_sdk_unofficial::app::*;
use firebase_rs_sdk_unofficial::auth::*;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    // Configure the Firebase project. Replace the placeholder values with your
    // project's credentials before running the example.
    let options = FirebaseOptions {
        api_key: Some("YOUR_WEB_API_KEY".into()),
        project_id: Some("your-project-id".into()),
        auth_domain: Some("your-project-id.firebaseapp.com".into()),
        ..Default::default()
    };

    let settings = FirebaseAppSettings {
        name: Some("auth-demo".into()),
        automatic_data_collection_enabled: Some(true),
    };

    // Initialise the core Firebase App instance.
    let app = initialize_app(options, Some(settings))?;

    // Ensure the Auth component is registered so `auth_for_app` succeeds.
    register_auth_component();
    let auth = auth_for_app(app.clone())?;

    // Replace these with credentials recognised by your Firebase project.
    let email = "alice@example.com";
    let password = "correct-horse-battery-staple";

    let credential = auth.sign_in_with_email_and_password(email, password)?;
    println!(
        "Signed in as {} (provider: {:?})",
        credential.user.uid(),
        credential.provider_id
    );

    if let Some(current_user) = auth.current_user() {
        println!(
            "Current user email: {:?}",
            current_user.info().email.clone()
        );
    }

    // Sign the user out and clean up the app instance when finished.
    auth.sign_out();
    println!("Signed out.");

    firebase_rs_sdk_unofficial::app::api::delete_app(&app)?;
    println!("App deleted.");

    Ok(())
}
```


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
- Ported initial account-management flows (`sendPasswordResetEmail`, `sendEmailVerification`, profile updates, re-auth, deletion) with mock-backed Rust tests (`src/auth/api.rs`).
- Extended coverage to provider unlinking and account lookup (`getAccountInfo`) to validate `accounts:update` and `accounts:lookup` interactions against the mock Identity Toolkit server.
- Added assertions for email/password update paths so token refresh and profile mutations mirror the JS test expectations.

### Current Status
- Core auth flows (sign-in/sign-up, password reset, email verification) now execute against configurable Identity Toolkit/Secure Token endpoints with deterministic mocks.
- Account mutation APIs (`accounts:update`, `accounts:delete`) persist refreshed tokens and propagate provider metadata, enabling unlink flows and account lookups to match JS semantics.
- Shared `test_support` fixtures allow any module to spin up isolated Firebase apps and `httpmock` servers, providing a template for the remaining API and core test ports.

### Next Focus
- **Account management completeness** – Port the remaining JS tests covering profile detail reads (`profile.test.ts`), email/password helpers (`email_and_password.test.ts`), and MFA enrollment/step-up flows (`mfa.test.ts`). Fill in any missing Rust helpers (e.g., `accounts:lookup` field parity, MFA endpoints) alongside new unit tests.
- **Authentication endpoints** – Translate the remaining REST suites (`create_auth_uri`, `custom_token`, `idp`, `sms`, `recaptcha`, `token`) by extending the mock server assertions introduced for password and account flows.
- **Core strategies & util** – Begin mapping `src/core/auth/*.test.ts` and `src/core/strategies/*.test.ts` to Rust unit tests, using the new helpers to stub token providers and persistence as needed.
- **Browser/Platform surfaces** – After core coverage stabilises, adapt the mock pattern for browser persistence/recaptcha tests (gated behind `wasm-web`) and outline any required feature flags or stubbed APIs.

Keep this roadmap updated as suites are ported: mark completed migrations, link Rust test modules, and note any design deviations from the JavaScript originals.
