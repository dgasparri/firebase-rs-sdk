# Example Usage

This document shares cookbook-style snippets that demonstrate how to plug
platform-specific code into the Auth module without bloating the core crate.
They are **not** production ready – adapt to your runtime and security model.

## 1. Provide Custom Persistence

```rust
use firebase-rs-sdk-unofficial-porting::auth::{
    Auth, AuthBuilder, ClosurePersistence, InMemoryRedirectPersistence, PersistedAuthState,
};
use firebase-rs-sdk-unofficial-porting::app::FirebaseApp;
use firebase-rs-sdk-unofficial-porting::auth::AuthResult;
use std::sync::Arc;

fn build_auth(app: FirebaseApp) -> AuthResult<Arc<Auth>> {
    let persistence = ClosurePersistence::new(
        |state: Option<PersistedAuthState>| {
            // TODO: write to your platform store (files, sqlite, etc.).
            Ok(())
        },
        || {
            // TODO: read from your platform store.
            Ok(None)
        },
    );

    let redirect_persistence = InMemoryRedirectPersistence::shared();

    Auth::builder(app)
        .with_persistence(Arc::new(persistence))
        .with_redirect_persistence(redirect_persistence)
        .build()
}
```

## 2. Wire a Popup Handler (WASM + JS)

Define a handler in Rust that forwards the popup request to host JavaScript.

```rust
use firebase-rs-sdk-unofficial-porting::auth::oauth::{
    OAuthPopupHandler, OAuthRequest,
};
use firebase-rs-sdk-unofficial-porting::auth::{Auth, AuthBuilder};
use firebase-rs-sdk-unofficial-porting::auth::model::AuthCredential;
use firebase-rs-sdk-unofficial-porting::auth::{AuthError, AuthResult};
use wasm_bindgen::prelude::*;
use std::sync::Arc;

struct WasmPopupHandler;

impl OAuthPopupHandler for WasmPopupHandler {
    fn open_popup(&self, request: OAuthRequest) -> AuthResult<AuthCredential> {
        let response_js = open_popup_via_js(&request.auth_url)?;
        let response = serde_wasm_bindgen::from_value(response_js)
            .map_err(|err| AuthError::InvalidCredential(err.to_string()))?;
        Ok(AuthCredential {
            provider_id: request.provider_id,
            sign_in_method: "oauth_popup".into(),
            token_response: response,
        })
    }
}

#[wasm_bindgen(module = "/js/auth_popup.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    fn open_popup_via_js(url: &str) -> Result<JsValue, JsValue>;
}

fn build_auth_with_popup(app: FirebaseApp) -> AuthResult<Arc<Auth>> {
    Auth::builder(app)
        .with_popup_handler(Arc::new(WasmPopupHandler))
        .build()
}
```

> Requires the optional `serde_wasm_bindgen` crate to bridge between `JsValue`
> and `serde_json::Value`.

Host-side JavaScript (referenced in the `wasm_bindgen` module) can implement
`open_popup_via_js` using the Firebase Web SDK logic you already own:

```javascript
// js/auth_popup.js
export async function open_popup_via_js(url) {
  const popup = window.open(url, '_blank', 'width=500,height=700');
  if (!popup) {
    throw new Error('Popup blocked');
  }
  // TODO: wait for postMessage, then return the serialized credential payload.
  return { idToken: '...', refreshToken: '...' };
}
```

## 3. Redirect Flow Skeleton

Applications drive redirect hand-offs with their own storage and routing.

```rust
use firebase-rs-sdk-unofficial-porting::auth::oauth::{
    OAuthRedirectHandler, OAuthRequest,
};
use firebase-rs-sdk-unofficial-porting::auth::model::AuthCredential;
use firebase-rs-sdk-unofficial-porting::auth::AuthResult;
use std::sync::Arc;

struct DesktopRedirectHandler;

impl OAuthRedirectHandler for DesktopRedirectHandler {
    fn initiate_redirect(&self, request: OAuthRequest) -> AuthResult<()> {
        // Persist state, open system browser, etc.
        launch_system_browser(&request.auth_url);
        Ok(())
    }

    fn complete_redirect(&self) -> AuthResult<Option<AuthCredential>> {
        // Inspect pending response (custom URL scheme, file, IPC channel...)
        Ok(None)
    }
}

fn launch_system_browser(url: &str) {
    // Implementation left to the application (shelling out, wry, tauri...).
}

fn build_auth_with_redirect(app: FirebaseApp) -> AuthResult<Arc<Auth>> {
    Auth::builder(app)
        .with_redirect_handler(Arc::new(DesktopRedirectHandler))
        .defer_initialization()
        .build()
}
```

> **Note**: The library does not attempt to embed webviews or tamper with the
> host environment. You control when/where to call the handler methods and what
> data they persist. This keeps the Rust port platform-agnostic while matching
> the Firebase JS SDK’s extension points.

## 4. Compose an OAuth Provider

```rust
use firebase-rs-sdk-unofficial-porting::auth::oauth::OAuthProvider;
use firebase-rs-sdk-unofficial-porting::auth::Auth;

fn google_provider() -> OAuthProvider {
    let mut provider = OAuthProvider::new(
        "google.com",
        "https://accounts.google.com/o/oauth2/v2/auth",
    );
    provider.add_scope("profile");
    provider.add_scope("email");
    provider
        .set_custom_parameters([
            ("prompt".to_string(), "select_account".to_string()),
        ]
        .into_iter()
        .collect());
    provider
}

fn request_popup_sign_in(auth: &Auth) {
    let provider = google_provider();
    let credential = provider
        .sign_in_with_popup(auth)
        .expect("popup handler must be registered");
    dbg!(credential.user.uid());
}

fn begin_redirect(auth: &Auth) {
    let provider = google_provider();
    provider
        .sign_in_with_redirect(auth)
        .expect("redirect handler must be registered");
}

fn complete_redirect(auth: &Auth) {
    if let Some(user_credential) = OAuthProvider::get_redirect_result(auth).unwrap() {
        dbg!(user_credential.user.uid());
    }
}

fn link_with_redirect(auth: &Auth) {
    let provider = google_provider();
    provider
        .link_with_redirect(auth)
        .expect("redirect handler must be registered");
}
```

## 5. Account Management Helpers

```rust
use firebase-rs-sdk-unofficial-porting::auth::model::AuthCredential;
use firebase-rs-sdk-unofficial-porting::auth::Auth;

fn send_reset(auth: &Auth) {
    auth.send_password_reset_email("user@example.com")
        .expect("password reset request");
}

fn verify_email(auth: &Auth) {
    auth.send_email_verification().expect("verification email");
}

fn update_profile(auth: &Auth) {
    auth
        .update_profile(Some("New Display"), Some("https://example.com/avatar.png"))
        .expect("profile update");
}

fn clear_photo(auth: &Auth) {
    // Passing an empty string clears the value.
    auth.update_profile(None, Some("")).expect("clear photo");
}

fn link_github(auth: &Auth, credential: AuthCredential) {
    auth
        .link_with_oauth_credential(credential)
        .expect("link credential");
}

fn reauth_password(auth: &Auth) {
    auth
        .reauthenticate_with_password("user@example.com", "hunter2")
        .expect("reauthenticate with password");
}

fn reauth_oauth(auth: &Auth, credential: AuthCredential) {
    auth
        .reauthenticate_with_oauth_credential(credential)
        .expect("reauthenticate with oauth");
}
```
