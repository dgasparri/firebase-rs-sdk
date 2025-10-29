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

- auth 82% `[########  ]`

==As of October 21th, 2025== 

Prompt: Compare the original JS/Typescript files in ./packages/auth and the ported files in Rust in ./src/auth, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust for this module

Porting Estimate

  - Roughly ~82% parity: Rust now covers email/password flows, custom-token and anonymous authentication, email link
  actions, account management, persistence, Secure Token refresh, and the complete multi-factor surface (phone, TOTP,
  passkey) including sign-in, link, and reauthentication flows. Outstanding work now focuses on browser ceremony helpers,
  tenant/emulator tooling, and richer provider ergonomics (packages/auth/src/api/index.ts:56,
  packages/auth/src/core/index.ts:292).

Coverage Highlights

  - Email/password sign-in & sign-up, custom token exchange, anonymous sessions, and email link sign-in share the same
  token management pipeline, keeping parity with the JS strategies while remaining async/wasm-friendly.
  - Phone number authentication now mirrors the JS confirmation flow (`signInWithPhoneNumber`, linking, reauth), using an
  async `ConfirmationResult` that plugs into pluggable application verifiers for reCAPTCHA/Play Integrity tokens.
  - Multi-factor enrollment for phone numbers reuses the same confirmation pipeline; `Auth::multi_factor()` exposes async
  helpers for creating sessions, enrolling factors, and unenrolling, matching the modular JS API shape. Passkey/WebAuthn
  and TOTP enrollment/sign-in are parity-complete, including resolver-driven sign-in, linking, and reauthentication.
  - Federated OAuth providers (Google, Facebook, GitHub, Twitter, Microsoft) include default scopes, PKCE support, and
  helper methods for popup/redirect flows plus link/reauth coverage.
  - `PhoneAuthProvider` mirrors the JS provider API, exposing verification helpers plus credential-based sign-in/link/reauth
  flows for SMS codes alongside the `sign_in_with_phone_number` convenience wrappers.
  - Phone and passkey MFA sign-in/link flows are covered end-to-end (`multi_factor_phone_enrollment_flow`,
    `passkey_multi_factor_link_flow`, etc.), providing the building blocks needed for higher-level resolver UX.
  - Out-of-band action helpers (`sendPasswordResetEmail`, `sendSignInLinkToEmail`, `applyActionCode`, `checkActionCode`,
  `verifyPasswordResetCode`) reuse a central request builder so both native and wasm targets can trigger Firebase emails
  without rewriting REST plumbing.
  - Account mutation APIs (profile/email/password updates, deletion, provider unlinking, reauthentication) remain feature
  complete relative to the JS REST surface, and listeners/persistence continue to mirror identity updates.
  - Persistence, Secure Token refresh, and OAuth scaffolding stay platform-agnostic, with IndexedDB-backed storage
  available behind the `wasm-web` feature for browser builds.
  - Extensive httpmock-backed unit tests exercise request payloads and token refresh logic to guard against regressions
  (requires loopback sockets when executed locally).

Major Gaps

  - Browser/Web ceremony helpers remain minimal: popup/redirect orchestration, conditional UI for passkeys, and
    reCAPTCHA/Play Integrity bootstrap still require consumer-provided glue despite hooks in the core APIs.
  - Advanced admin surfaces such as tenant-aware auth, emulator toggles, localization helpers, password-policy endpoints,
    and token revocation/session cookie flows remain unported.
  - Provider ergonomics beyond the REST primitives—credential serialization, full OAuth provider builders, and durable
    redirect storage integrations—lag behind the JS SDK.

Next Steps

  1. **Browser bridge crates** – Deliver popup/redirect + conditional UI adapters for WASM targets so OAuth and passkey
     flows operate with minimal consumer glue (including reCAPTCHA/Play Integrity bootstrapping).
  2. **Tenant/emulator & policy endpoints** – Surface project configuration, password policy, token revocation, and
     emulator toggles with rustdoc’d APIs and examples, ensuring they interoperate with existing MFA resolvers.
  3. **Provider ergonomics** – Finish porting provider helpers (Google/Facebook/etc.), credential serializers, and session
     cookie utilities to reach the JS SDK developer experience across platforms.
  4. **Testing/documentation sweep** – Port the remaining JS browser/resolver suites and expand docs to cover advanced
     scenarios (emulators, multi-tenant usage, popup/redirect best practices).



## References to the Firebase JS SDK - firestore module

- QuickStart: <https://firebase.google.com/docs/auth/web/start>
- API: <https://firebase.google.com/docs/reference/js/auth.md#auth_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/auth>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/auth>

## Development status as of 14th October 2025

- Core functionalities: Mostly implemented (see the module's [README.md](https://github.com/dgasparri/firebase-rs-sdk/tree/main/src/auth) for details)
- Tests: httpmock-backed unit suite expanded (requires loopback binding when run locally)
- Documentation: Most public functions are documented
- Examples: 1 provided

DISCLAIMER: This is not an official Firebase product, nor it is guaranteed that it has no bugs or that it will work as intended.

## Example Usage

```rust
use firebase_rs_sdk::app::*;
use firebase_rs_sdk::auth::*;
use std::error::Error;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
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
    let app = initialize_app(options, Some(settings)).await?;

    // Ensure the Auth component is registered so `auth_for_app` succeeds.
    register_auth_component();
    let auth = auth_for_app(app.clone())?;

    // Replace these with credentials recognised by your Firebase project.
    let email = "alice@example.com";
    let password = "correct-horse-battery-staple";

    let credential = auth
        .sign_in_with_email_and_password(email, password)
        .await?;
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

    firebase_rs_sdk::app::api::delete_app(&app).await?;
    println!("App deleted.");

    Ok(())
}
```

> **Runtime note:** The sample uses `tokio` for native async execution. On WASM, drive these futures with
> `wasm-bindgen-futures::spawn_local` or the surrounding host instead of `tokio`.


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
  - Email/password, custom-token, anonymous, and email-link flows share the same token finalisation path, covering
    `signInWithPassword`, `signUp`, `signInWithCustomToken`, and `signInWithEmailLink` while remaining async-friendly for
    wasm builds.
  - `signInWithIdp` REST exchange for OAuth credentials produced by popup/redirect handlers.
  - Unified helper for out-of-band actions (`sendOobCode`, `resetPassword`, `applyActionCode`) keeps email resets,
    verification, and email link flows consistent across platforms.
- **Account management APIs** (`api/account.rs`)
  - Password reset, email verification, profile/email/password updates, user deletion, and provider linking are wired via
    the Firebase Auth REST endpoints, surfaced through new `Auth` helpers. Re-authentication helpers cover both password
    and OAuth credentials.
- **Multi-factor APIs** (`api/core/mfa.rs`, `types.rs`)
  - Phone-based second-factor enrollment reuses the confirmation pipeline; `Auth::multi_factor()` returns a
    `MultiFactorUser` with async helpers for session creation, enrollment, factor inspection, and unenrollment.
  - Step-up sign-in surfaces `MultiFactorResolver`, exposing factor hints, captured sessions, verification helpers, and
    resolver-driven completion for phone challenges raised via `mfaPendingCredential` responses across sign-in,
    reauthentication, and credential linking flows.
  - Passkey/WebAuthn factors provide typed challenge retrieval (`WebAuthnSignInChallenge`) and assertion finalisation via
    `WebAuthnMultiFactorGenerator`, and now cover both sign-in and enrollment flows through
    `MultiFactorResolver::start_passkey_sign_in` and `MultiFactorUser::start_passkey_enrollment`.
  - WebAuthn metadata (allow credentials, authenticator transports, attestation public keys) is exposed via typed
    accessors on `WebAuthnSignInChallenge`, `WebAuthnAssertionResponse`, and `WebAuthnAttestationResponse`, matching the
    modular JS surface.
  - Multi-factor hints mirror the JS ordering by sorting enrolled factors by their `enrolledAt` timestamp while preserving
    server display names for clearer resolver UX.
  - Helper methods such as `challenge.challenge_bytes()` and `response.with_signature(...)` simplify the bridge between
    browser WebAuthn APIs and `MultiFactorResolver::resolve_sign_in`. See `examples/auth_passkey_roundtrip.rs` for a
    complete passkey resolver round trip.
  - TOTP enrollment/sign-in flows are supported via `TotpMultiFactorGenerator`, including secret generation helpers and
    resolver integration during multi-factor sign-in.
- **Phone provider utilities** (`phone/`)
  - `PhoneAuthProvider` and `PhoneAuthCredential` offer low-level verification helpers alongside credential-based
    sign-in/link/reauth flows, mirroring the Firebase JS provider ergonomics for SMS authentication.
- **Models & types** (`model.rs`, `types.rs`)
  - User models (`User`, `UserCredential`), provider structs (`EmailAuthProvider`), action code types, token metadata.
- **Errors & result handling** (`error.rs`)
  - Auth-specific error enumeration and result aliases, plus typed multi-factor error codes that surface
    `missing-multi-factor-session`/`multi-factor-info-not-found` as structured Rust variants.
- **Module wiring** (`mod.rs`)
  - Public re-exports and component registration entrypoints used by other services.
- **Token refresh scaffolding** (`token_manager.rs`, `api/token.rs`, `token_provider.rs`)
  - Tracks ID/refresh tokens with expiry metadata, hits the Secure Token endpoint on demand, and exposes an
    `AuthTokenProvider` that other services (Firestore, Functions) can plug in for authenticated requests.
- **State persistence (in-memory + web storage)** (`persistence/`)
  - Saves/restores the signed-in user and tokens across restarts via a pluggable persistence interface. Native builds
    default to an in-memory driver, while WASM targets can enable the `wasm-web` feature to use local/session storage
    with multi-tab coordination through the new `WebStoragePersistence` adapter.

This functionality now covers email/password, custom token, anonymous, email-link, and phone sign-in flows with shared
token handling, enabling dependent modules to retrieve `Auth` instances across native and wasm targets.

## Gaps vs `packages/auth`

The JavaScript implementation is significantly broader. Missing pieces include:

1. **Federated provider ergonomics**
   - OAuth providers (Google, Facebook, GitHub, etc.) still require provider-specific helpers, popup/redirect orchestration,
     and PKCE/account-linking nuances from the JS SDK.
2. **Browser & hybrid platform adapters**
   - Cordova, React Native, and browser-specific glue (navigation messaging, reCAPTCHA/Play Integrity bootstrap,
     IndexedDB-backed persistence) are still missing; consumers must currently wire their own handlers.
4. **Advanced account & tenant tooling**
   - Tenant management, localization, emulator hooks, and password-policy/project-configuration endpoints have not yet
     been ported.
5. **Secure token & policy endpoints**
   - Token revocation, session cookie management, custom claims, and related REST surfaces still need Rust wrappers and
     error translation.
6. **Testing parity**
   - Extensive JS unit/integration suites (providers, MFA, browser strategies) still need Rust equivalents.

## Next Steps

1. **Federated providers**
    - Finish the OAuth provider implementations (Google/Facebook/GitHub/etc.), including popup/redirect orchestration,
      PKCE support, and credential/linking helpers.
2. **Browser & hybrid adapters**
    - Provide concrete implementations for popup/redirect handlers, reCAPTCHA/Play Integrity bootstrap, and persistence
      quirks across web, React Native, and Cordova environments.
3. **Tenant/emulator & policy endpoints**
    - Surface project configuration, password policy, token revocation, and emulator-friendly toggles with rustdoc'd APIs
      and associated error mapping.
4. **Persistence & lifecycle**
    - Add IndexedDB/native durable storage backends and flesh out proactive refresh observers (`beforeAuthStateChanged`).
5. **Testing**
    - Translate the remaining JS suites (providers, MFA, browser flows) to Rust, reusing the `httpmock` harness across
      modules.

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
- **Authentication endpoints** – Translate the remaining REST suites (`create_auth_uri`, `idp`, `recaptcha`, `token`, `project_config`) by extending the mock server assertions introduced for password and account flows.
- **Core strategies & util** – Begin mapping `src/core/auth/*.test.ts` and `src/core/strategies/*.test.ts` to Rust unit tests, using the new helpers to stub token providers and persistence as needed.
- **Browser/Platform surfaces** – After core coverage stabilises, adapt the mock pattern for browser persistence/recaptcha tests (gated behind `wasm-web`) and outline any required feature flags or stubbed APIs.

Keep this roadmap updated as suites are ported: mark completed migrations, link Rust test modules, and note any design deviations from the JavaScript originals.
