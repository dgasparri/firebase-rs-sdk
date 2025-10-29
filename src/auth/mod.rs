#![doc = include_str!("README.md")]
pub mod api;
pub mod error;
pub mod model;
pub mod oauth;
pub mod persistence;
pub mod phone;
mod token_manager;
#[cfg(all(not(target_arch = "wasm32"), feature = "firestore"))]
pub mod token_provider;
pub mod types;

#[doc(inline)]
pub use api::{auth_for_app, register_auth_component, Auth, AuthBuilder};

#[doc(inline)]
pub use error::{AuthError, AuthResult, MultiFactorAuthError, MultiFactorAuthErrorCode};

#[doc(inline)]
pub use model::{AuthConfig, AuthCredential, EmailAuthProvider, User, UserCredential};

#[doc(inline)]
pub use phone::{
    PhoneAuthCredential, PhoneAuthProvider, PhoneMultiFactorGenerator, PHONE_PROVIDER_ID,
};

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
