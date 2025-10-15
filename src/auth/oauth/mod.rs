use std::collections::HashMap;

use crate::auth::error::AuthResult;
use crate::auth::model::AuthCredential;

/// Parameters needed to initiate an OAuth identity provider flow.
///
/// Consumers construct the final authorization URL using the provided endpoint
/// and parameters. These values mirror the Firebase JS SDK `AuthEventManager`
/// inputs, allowing a 1:1 translation for popup and redirect handlers.
#[derive(Debug, Clone)]
pub struct OAuthRequest {
    /// Provider identifier (e.g. `google.com`).
    pub provider_id: String,
    /// Fully qualified authorization URL.
    pub auth_url: String,
    /// Optional human-readable hint to display in custom UI.
    pub display_name: Option<String>,
    /// Optional locale hint.
    pub language_code: Option<String>,
    /// Additional query parameters to include when opening the provider.
    pub custom_parameters: HashMap<String, String>,
}

impl OAuthRequest {
    pub fn new(provider_id: impl Into<String>, auth_url: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            auth_url: auth_url.into(),
            display_name: None,
            language_code: None,
            custom_parameters: HashMap::new(),
        }
    }

    pub fn with_display_name(mut self, value: impl Into<String>) -> Self {
        self.display_name = Some(value.into());
        self
    }

    pub fn with_language_code(mut self, value: impl Into<String>) -> Self {
        self.language_code = Some(value.into());
        self
    }

    pub fn with_custom_parameters(mut self, parameters: HashMap<String, String>) -> Self {
        self.custom_parameters = parameters;
        self
    }
}

/// Handles OAuth popup operations for interactive sign-in flows.
///
/// Implementations should open a browser window/dialog, complete the
/// authorization handshake, and return an [`crate::auth::model::AuthCredential`] produced from the
/// provider response. The handler is free to block the current thread or spawn
/// an async task; the library does not impose scheduling requirements.
pub trait OAuthPopupHandler: Send + Sync {
    fn open_popup(&self, request: OAuthRequest) -> AuthResult<AuthCredential>;
}

/// Handles OAuth redirect-based flows.
///
/// Redirect flows require two phases:
/// 1. Call `initiate_redirect` before leaving the current context.
/// 2. After the application reloads/returns, call `complete_redirect` to
///    resolve the awaited credential.
pub trait OAuthRedirectHandler: Send + Sync {
    fn initiate_redirect(&self, request: OAuthRequest) -> AuthResult<()>;
    fn complete_redirect(&self) -> AuthResult<Option<AuthCredential>>;
}

pub mod credential;
pub mod provider;
pub mod providers;
pub mod redirect;

pub use credential::OAuthCredential;
pub use provider::OAuthProvider;
pub use providers::*;
pub use redirect::{
    InMemoryRedirectPersistence, PendingRedirectEvent, RedirectOperation, RedirectPersistence,
};
