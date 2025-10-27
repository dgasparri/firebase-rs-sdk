//! # Firebase Authentication module
//!
//! This module ports core pieces of the Firebase Authentication SDK to Rust.
//!
//! It provides functionality to interact with Firebase Authentication, allowing users to sign in to your app using one or more sign-in methods, including email address and password sign-in, and federated identity providers such as Google Sign-in and Facebook Login. Through the SDK it is also possible to manage the user's credentials.
//!
//! It includes error handling, configuration options, and integration with Firebase apps.
//!
//! ## Features
//!
//! - Initialize Auth for any FirebaseApp, with optional custom persistence.
//! - Sign users in/out via email + password, IDP (OAuth) flows, and custom tokens once implemented.
//! - Observe auth state changes and access the current User snapshot.
//! - Refresh and manage ID tokens, exposing them through the AuthTokenProvider bridge for other services.
//! - Access provider helpers (e.g., EmailAuthProvider) and user profile updates, matching the JS API ergonomics.
//!
//! ## References to the Firebase JS SDK - firestore module
//!
//! - QuickStart: <https://firebase.google.com/docs/auth/web/start>
//! - API: <https://firebase.google.com/docs/reference/js/auth.md#auth_package>
//! - Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/auth>
//! - Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/auth>
//!
//! ## Development status as of 14th October 2025
//!
//! - Core functionalities: Mostly implemented (see the module's [README.md](https://github.com/dgasparri/firebase-rs-sdk/tree/main/src/auth) for details)
//! - Tests: 30 tests (passed)
//! - Documentation: Most public functions are documented
//! - Examples: 1 provided
//!
//! DISCLAIMER: This is not an official Firebase product, nor it is guaranteed that it has no bugs or that it will work as intended.
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use firebase_rs_sdk::app::*;
//! use firebase_rs_sdk::auth::*;
//! use std::error::Error;
//!
//! async fn main() -> Result<(), Box<dyn Error>> {
//!     // Configure the Firebase project. Replace the placeholder values with your
//!     // project's credentials before running the example.
//!     let options = FirebaseOptions {
//!         api_key: Some("YOUR_WEB_API_KEY".into()),
//!         project_id: Some("your-project-id".into()),
//!         auth_domain: Some("your-project-id.firebaseapp.com".into()),
//!         ..Default::default()
//!     };
//!
//!     let settings = FirebaseAppSettings {
//!         name: Some("auth-demo".into()),
//!         automatic_data_collection_enabled: Some(true),
//!     };
//!
//!     // Initialise the core Firebase App instance.
//!     let app = initialize_app(options, Some(settings)).await?;
//!
//!     // Ensure the Auth component is registered so `auth_for_app` succeeds.
//!     register_auth_component();
//!     let auth = auth_for_app(app.clone())?;
//!
//!     // Replace these with credentials recognised by your Firebase project.
//!     let email = "alice@example.com";
//!     let password = "correct-horse-battery-staple";
//!
//!     let credential = auth.sign_in_with_email_and_password(email, password).await?;
//!     println!(
//!         "Signed in as {} (provider: {:?})",
//!         credential.user.uid(),
//!         credential.provider_id
//!     );
//!
//!     if let Some(current_user) = auth.current_user() {
//!         println!(
//!             "Current user email: {:?}",
//!             current_user.info().email.clone()
//!         );
//!     }
//!
//!     // Sign the user out and clean up the app instance when finished.
//!     auth.sign_out();
//!     println!("Signed out.");
//!
//!     firebase_rs_sdk::app::api::delete_app(&app).await?;
//!     println!("App deleted.");
//!
//!     Ok(())
//! }
//! ```

pub mod api;
pub mod error;
pub mod model;
pub mod oauth;
pub mod persistence;
mod token_manager;
#[cfg(all(not(target_arch = "wasm32"), feature = "firestore"))]
pub mod token_provider;
pub mod types;

#[doc(inline)]
pub use api::{auth_for_app, register_auth_component, Auth, AuthBuilder};

#[doc(inline)]
pub use error::{AuthError, AuthResult};

#[doc(inline)]
pub use model::{AuthConfig, AuthCredential, EmailAuthProvider, User, UserCredential};

#[doc(inline)]
pub use oauth::{
    OAuthCredential, OAuthPopupHandler, OAuthProvider, OAuthRedirectHandler, OAuthRequest,
};

#[doc(inline)]
pub use persistence::{
    AuthPersistence, ClosurePersistence, InMemoryPersistence, PersistedAuthState,
};

#[cfg(all(not(target_arch = "wasm32"), feature = "firestore"))]
#[doc(inline)]
pub use token_provider::AuthTokenProvider;

#[doc(inline)]
pub use types::*;
