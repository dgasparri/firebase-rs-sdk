pub mod api;
pub mod error;
pub mod model;
pub mod oauth;
pub mod persistence;
mod token_manager;
pub mod token_provider;
pub mod types;

pub use api::{Auth, AuthBuilder};
pub use error::{AuthError, AuthResult};
pub use model::{AuthConfig, EmailAuthProvider, User, UserCredential};
pub use oauth::{
    OAuthCredential, OAuthPopupHandler, OAuthProvider, OAuthRedirectHandler, OAuthRequest,
};
pub use persistence::{
    AuthPersistence, ClosurePersistence, InMemoryPersistence, PersistedAuthState,
};
pub use token_provider::AuthTokenProvider;
pub use types::*;
