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

## Porting Status

- auth 85% `[######### ]`

- Core functionalities: Mostly implemented
- Tests: httpmock-backed unit suite expanded (requires loopback binding when run locally)
- Documentation: Most public functions are documented
- Examples: 4 provided

Based on the current codebase, I’d put parity with the Firebase Auth JS SDK at roughly 85 %. All of the core sign-in flows (email/password, custom token, anonymous, phone, TOTP, passkey), multi-factor resolver/linking, OAuth scaffolding (with PKCE and built-in providers), persistence, and token refresh logic are in place. The remaining  gap is mostly around browser-specific ceremony glue (popup/redirect adapters, conditional UI), advanced admin/tenant tooling, and a handful of higher-level provider conveniences that the JS SDK ships out of the box.

## References to the Firebase JS SDK - firestore module

- QuickStart: <https://firebase.google.com/docs/auth/web/start>
- API: <https://firebase.google.com/docs/reference/js/auth.md#auth_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/auth>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/auth>


## Example Usage

```rust,no_run
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

    firebase_rs_sdk::app::delete_app(&app).await?;
    println!("App deleted.");

    Ok(())
}
```

> **Runtime note:** The sample uses `tokio` for native async execution. On WASM, drive these futures with
> `wasm-bindgen-futures::spawn_local` or the surrounding host instead of `tokio`.


## What's Implemented

- **Auth service core** (`api.rs`, `mod.rs`) provides component registration, `Auth::builder`, and integration with the app provider registry so callers can resolve `Auth` instances.
- **OAuth scaffolding** (`oauth/`) defines `OAuthRequest`, popup/redirect handler traits, provider builders with PKCE support, and redirect persistence hooks alongside native/WASM examples.
- **REST auth flows** (`api.rs`) unify email/password, custom token, anonymous, email link, and IdP exchanges and centralise out-of-band actions for password reset, email verification, and email link delivery.
- **Account management** (`api/account.rs`) surfaces profile/email/password updates, provider link/unlink, reauthentication helpers, and user deletion endpoints via the Auth REST API.
- **Multi-factor support** (`api/core/mfa.rs`, `types.rs`) covers phone, passkey/WebAuthn, and TOTP enrollment and sign-in with resolver utilities, typed challenges, and session helpers.
- **Phone provider utilities** (`phone/`) supply `PhoneAuthProvider`, `PhoneAuthCredential`, and pluggable verifiers that mirror the JS confirmation flows for SMS authentication.
- **Models & credential helpers** (`model.rs`, `types.rs`) expose `User`, `UserCredential`, provider structs, action code types, token metadata, and JSON serialization helpers for credentials.
- **Error handling** (`error.rs`) defines auth-specific error enums, including multi-factor variants, and result aliases used throughout the module.
- **Token management** (`token_manager.rs`, `api/token.rs`, `token_provider.rs`) tracks ID/refresh tokens, calls the Secure Token endpoint, and provides the `AuthTokenProvider` bridge for other services.
- **State persistence** (`persistence/`) offers in-memory and web storage drivers, including `WebStoragePersistence` for WASM builds behind the `wasm-web` feature.
- **Testing coverage** (unit tests under `src/auth`) uses `httpmock` to exercise REST payloads, persistence, and token refresh logic, guarding against regressions.


### Remaining Gaps

1. **Browser ceremony adapters** – Ship first-party popup/redirect bridges (including conditional passkey UI and
   reCAPTCHA/Play Integrity bootstrap) so WASM/browser consumers get turnkey flows.
2. **Tenant & emulator tooling** – Port tenant-aware auth, localization hooks, password-policy/token-revocation/session cookie endpoints,
   and emulator-friendly toggles.
3. **Provider UX polish** – Add higher-level helpers for remaining providers (Apple/Yahoo variations, Apple nonce
   management), credential serialization shortcuts for cross-process replay, and richer redirect persistence utilities.
4. **Testing & docs parity** – Expand browser/resolver test coverage and documentation to mirror the JS SDK guidance.

### Next Steps

1. **Browser bridge crates** – Deliver popup/redirect + conditional UI adapters for WASM targets so OAuth and passkey
   flows operate with minimal consumer glue (including reCAPTCHA/Play Integrity bootstrapping).
2. **Tenant/emulator & policy endpoints** – Surface project configuration, password policy, token revocation, and
   emulator toggles with rustdoc’d APIs and examples, ensuring they interoperate with existing MFA resolvers.
3. **Provider ergonomics** – Finish porting provider helpers (Google/Facebook/etc.), credential serializers, and session
   cookie utilities to reach the JS SDK developer experience across platforms.
4. **Testing/documentation sweep** – Port the remaining JS browser/resolver suites and expand docs to cover advanced
   scenarios (emulators, multi-tenant usage, popup/redirect best practices).
5. **Testing**
    - Translate the remaining JS suites (providers, MFA, browser flows) to Rust, reusing the `httpmock` harness across
      modules.


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
