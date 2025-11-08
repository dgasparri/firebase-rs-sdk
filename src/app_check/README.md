# Firebase App Check

This module ports Firebase App Check to Rust so client code can obtain, cache, and refresh attestation tokens that protect backend resources from abuse. The Rust port mirrors the modular JS SDK: it exposes an App Check façade, provider implementations, proactive refresh scheduling, IndexedDB persistence for wasm builds, and an internal bridge that other services (Firestore, Storage, etc.) can depend on.

Porting status: 70% `[#######   ]` ([details](https://github.com/dgasparri/firebase-rs-sdk/blob/main/src/app_check/PORTING_STATUS.md))


## Quick Start Example

```rust,no_run
use firebase_rs_sdk::app::{initialize_app, FirebaseOptions};
use firebase_rs_sdk::app_check::{initialize_app_check, custom_provider, token_with_ttl, AppCheckOptions};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Initialise a Firebase app (see the `app` module for full options).
    let app = initialize_app(FirebaseOptions::default(), None).await.unwrap();

    // Register a simple custom App Check provider.
    let provider = custom_provider(|| token_with_ttl("dev-token", Duration::from_secs(300)));
    let options = AppCheckOptions::new(provider);
    let app_check = initialize_app_check(Some(app.clone()), options).await.unwrap();

    // Fetch and refresh tokens as needed.
    let token = app_check.get_token(false).await.expect("Could not get token");
    println!("token={}", token.token);

}
```

For web/WASM builds you can swap the custom provider for
`recaptcha_v3_provider("your-site-key")` or
`recaptcha_enterprise_provider("your-enterprise-site-key")`; the helper manages
script injection, invisible widget lifecycle, and backend throttling exactly as
the JS SDK does.

## References to the Firebase JS SDK

- QuickStart: <https://firebase.google.com/docs/app-check/web/recaptcha-provider>
- API: <https://firebase.google.com/docs/reference/js/app-check.md#app-check_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/app-check>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/app-check>

## Intentional deviations from the JS SDK

- **No dummy-token fallback** – The JS SDK always resolves `getToken()` with a string, returning a base64 "dummy" token alongside error metadata when the exchange fails. Rust callers already rely on `Result`, so the port surfaces enriched error variants instead of fabricating placeholder tokens. This keeps downstream code explicit while still exposing throttling/backoff details through the returned error value.

