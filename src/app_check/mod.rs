//! # Firebase Firestore module
//!
//! This module ports core pieces of the Firebase App Check SDK to Rust so applications can request, cache, and refresh attestation tokens that protect backend resources from abuse. It mirrors the JS SDK’s structure with an App Check façade, provider implementations, and internal wiring that other services (Firestore, Storage, etc.) can tap into via token providers.
//!
//! It includes error handling, configuration options, and integration with Firebase apps.
//!
//! ## Features
//!
//! - Initialize App Check for any FirebaseApp, choosing between the built-in reCAPTCHA providers or a custom provider callback.
//! - Toggle automatic token refresh and listen for token updates through observer APIs.
//! - Retrieve standard and limited-use App Check tokens on demand, receiving structured error details when attestation fails.
//! - Bridge App Check tokens into dependent services via FirebaseAppCheckInternal::token_provider so HTTP clients can attach X-Firebase-AppCheck headers automatically.
//! - Manage internal listeners (add/remove) and inspect cached token state for emulator or server-driven scenarios.
//!
//! ## References to the Firebase JS SDK - firestore module
//!
//! - QuickStart: <https://firebase.google.com/docs/app-check/web/recaptcha-provider>
//! - API: <https://firebase.google.com/docs/reference/js/app-check.md#app-check_package>
//! - Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/app-check>
//! - Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/app-check>
//!
//! ## Development status as of 14th October 2025
//!
//! - Core functionalities: Some implemented (see the module's [README.md](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/app_check) for details)
//! - Tests: 4 tests (passed)
//! - Documentation: Some public functions are documented
//! - Examples: None provided
//!
//! DISCLAIMER: This is not an official Firebase product, nor it is guaranteed that it has no bugs or that it will work as intended.
//!
//! ## Example Usage
//!
//! ```rust
//! use firebase_rs_sdk_unofficial::app::api::{delete_app, initialize_app};
//! use firebase_rs_sdk_unofficial::app::{FirebaseAppSettings, FirebaseOptions};
//! use firebase_rs_sdk_unofficial::app_check::api::{
//!     add_token_listener, custom_provider, get_limited_use_token, get_token, initialize_app_check,
//!     set_token_auto_refresh_enabled, token_with_ttl,
//! };
//! use firebase_rs_sdk_unofficial::app_check::{AppCheckOptions, AppCheckTokenListener, ListenerType};
//! use std::error::Error;
//! use std::sync::Arc;
//! use std::time::Duration;
//!
//! fn main() -> Result<(), Box<dyn Error>> {
//!     // Configure the Firebase project. Replace these placeholder values with your
//!     // own Firebase configuration when running the sample against real services.
//!     let options = FirebaseOptions {
//!         api_key: Some("YOUR_WEB_API_KEY".into()),
//!         project_id: Some("your-project-id".into()),
//!         app_id: Some("1:1234567890:web:abcdef".into()),
//!         ..Default::default()
//!     };
//!
//!     let settings = FirebaseAppSettings {
//!         name: Some("app-check-demo".into()),
//!         automatic_data_collection_enabled: Some(true),
//!     };
//!
//!     let app = initialize_app(options, Some(settings))?;
//!
//!     // Create a simple provider that always returns the same demo token.
//!     let provider = custom_provider(|| token_with_ttl("demo-app-check", Duration::from_secs(60)));
//!     let options = AppCheckOptions::new(provider.clone()).with_auto_refresh(true);
//!
//!     let app_check = initialize_app_check(Some(app.clone()), options)?;
//!
//!     // Enable or disable automatic background refresh.
//!     set_token_auto_refresh_enabled(&app_check, true);
//!
//!     // Listen for token updates. The listener fires immediately with the cached token
//!     // (if any) and then on subsequent refreshes.
//!     let listener: AppCheckTokenListener = Arc::new(|result| {
//!         if !result.token.is_empty() {
//!             println!("Received App Check token: {}", result.token);
//!         }
//!         if let Some(error) = &result.error {
//!             eprintln!("App Check token error: {error}");
//!         }
//!     });
//!     let handle = add_token_listener(&app_check, listener, ListenerType::External)?;
//!
//!     // Retrieve the current token and a limited-use token.
//!     let token = get_token(&app_check, false)?;
//!     println!("Immediate token fetch: {}", token.token);
//!
//!     let limited = get_limited_use_token(&app_check)?;
//!     println!("Limited-use token: {}", limited.token);
//!
//!     // Listener handles implement Drop and automatically unsubscribe, but you can
//!     // explicitly disconnect if desired.
//!     handle.unsubscribe();
//!
//!     delete_app(&app)?;
//!     Ok(())
//! }
//! ```

pub mod api;
mod errors;
mod interop;
mod logger;
mod providers;
mod state;
mod token_provider;
mod types;

pub use api::*;
pub use errors::{AppCheckError, AppCheckResult};
pub use interop::FirebaseAppCheckInternal;
pub use providers::{
    CustomProvider, CustomProviderOptions, ReCaptchaEnterpriseProvider, ReCaptchaV3Provider,
};
pub use token_provider::{app_check_token_provider_arc, AppCheckTokenProvider};
pub use types::{
    AppCheck, AppCheckInternalListener, AppCheckOptions, AppCheckProvider, AppCheckToken,
    AppCheckTokenListener, AppCheckTokenResult, ListenerHandle, ListenerType,
    APP_CHECK_COMPONENT_NAME, APP_CHECK_INTERNAL_COMPONENT_NAME,
};
