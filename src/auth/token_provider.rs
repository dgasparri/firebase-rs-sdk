use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::auth::error::AuthError;
use crate::auth::Auth;
use crate::firestore::error::{
    internal_error, unauthenticated, unavailable, FirestoreError, FirestoreResult,
};
use crate::firestore::remote::datastore::{TokenProvider, TokenProviderArc};

pub struct AuthTokenProvider {
    auth: Arc<Auth>,
    force_refresh: AtomicBool,
}

impl AuthTokenProvider {
    /// Creates a Firestore-compatible token provider backed by Firebase Auth.
    pub fn new(auth: Arc<Auth>) -> Self {
        Self {
            auth,
            force_refresh: AtomicBool::new(false),
        }
    }

    /// Converts the provider into an `Arc` for Datastore integration.
    pub fn into_arc(self) -> TokenProviderArc {
        Arc::new(self)
    }
}

impl Clone for AuthTokenProvider {
    fn clone(&self) -> Self {
        Self {
            auth: self.auth.clone(),
            force_refresh: AtomicBool::new(self.force_refresh.load(Ordering::SeqCst)),
        }
    }
}

impl TokenProvider for AuthTokenProvider {
    fn get_token(&self) -> FirestoreResult<Option<String>> {
        let force_refresh = self.force_refresh.swap(false, Ordering::SeqCst);
        self.auth.get_token(force_refresh).map_err(map_auth_error)
    }

    fn invalidate_token(&self) {
        self.force_refresh.store(true, Ordering::SeqCst);
    }
}

fn map_auth_error(error: AuthError) -> FirestoreError {
    match error {
        AuthError::InvalidCredential(message) => unauthenticated(message),
        AuthError::Network(message) => unavailable(message),
        AuthError::Firebase(firebase_error) => unauthenticated(firebase_error.message),
        AuthError::App(app_error) => internal_error(app_error.to_string()),
        AuthError::NotImplemented(feature) => {
            internal_error(format!("{feature} is not implemented"))
        }
    }
}

/// Convenience helper that wraps an `Auth` instance into a token provider arc.
pub fn auth_token_provider_arc(auth: Arc<Auth>) -> TokenProviderArc {
    AuthTokenProvider::new(auth).into_arc()
}
