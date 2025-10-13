use std::sync::atomic::{AtomicBool, Ordering};

use crate::app_check::errors::AppCheckError;
use crate::app_check::FirebaseAppCheckInternal;
use crate::firestore::error::{
    internal_error, invalid_argument, unauthenticated, unavailable, FirestoreError, FirestoreResult,
};
use crate::firestore::remote::datastore::{TokenProvider, TokenProviderArc};

/// Bridges App Check token retrieval into Firestore's [`TokenProvider`] trait.
pub struct AppCheckTokenProvider {
    app_check: FirebaseAppCheckInternal,
    force_refresh: AtomicBool,
}

impl AppCheckTokenProvider {
    /// Creates a new provider backed by the given App Check instance.
    pub fn new(app_check: FirebaseAppCheckInternal) -> Self {
        Self {
            app_check,
            force_refresh: AtomicBool::new(false),
        }
    }

    /// Converts the provider into a reference-counted [`TokenProviderArc`].
    pub fn into_arc(self) -> TokenProviderArc {
        std::sync::Arc::new(self)
    }
}

/// Convenience helper to expose an App Check instance as a [`TokenProviderArc`].
pub fn app_check_token_provider_arc(app_check: FirebaseAppCheckInternal) -> TokenProviderArc {
    AppCheckTokenProvider::new(app_check).into_arc()
}

impl Clone for AppCheckTokenProvider {
    fn clone(&self) -> Self {
        Self {
            app_check: self.app_check.clone(),
            force_refresh: AtomicBool::new(self.force_refresh.load(Ordering::SeqCst)),
        }
    }
}

impl TokenProvider for AppCheckTokenProvider {
    fn get_token(&self) -> FirestoreResult<Option<String>> {
        let force_refresh = self.force_refresh.swap(false, Ordering::SeqCst);
        let result = self
            .app_check
            .get_token(force_refresh)
            .map_err(map_app_check_error)?;

        if let Some(error) = result.error {
            return Err(map_app_check_error(error));
        }

        if let Some(error) = result.internal_error {
            return Err(internal_error(error.to_string()));
        }

        if result.token.is_empty() {
            Ok(None)
        } else {
            Ok(Some(result.token))
        }
    }

    fn invalidate_token(&self) {
        self.force_refresh.store(true, Ordering::SeqCst);
    }
}

fn map_app_check_error(error: AppCheckError) -> FirestoreError {
    match error {
        AppCheckError::AlreadyInitialized { .. }
        | AppCheckError::UseBeforeActivation { .. }
        | AppCheckError::InvalidConfiguration { .. } => invalid_argument(error.to_string()),
        AppCheckError::TokenFetchFailed { .. } | AppCheckError::ProviderError { .. } => {
            unavailable(error.to_string())
        }
        AppCheckError::TokenExpired => unauthenticated(error.to_string()),
        AppCheckError::Internal(message) => internal_error(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{FirebaseApp, FirebaseAppConfig, FirebaseOptions};
    use crate::app_check::api::{initialize_app_check, token_with_ttl};
    use crate::app_check::types::{AppCheckOptions, AppCheckProvider, AppCheckToken};
    use crate::component::ComponentContainer;
    use std::sync::Arc;
    use std::time::Duration;

    #[derive(Clone)]
    struct StaticTokenProvider {
        token: String,
    }

    impl AppCheckProvider for StaticTokenProvider {
        fn get_token(&self) -> crate::app_check::AppCheckResult<AppCheckToken> {
            token_with_ttl(self.token.clone(), Duration::from_secs(60))
        }
    }

    #[derive(Clone)]
    struct ErrorProvider;

    impl AppCheckProvider for ErrorProvider {
        fn get_token(&self) -> crate::app_check::AppCheckResult<AppCheckToken> {
            Err(AppCheckError::TokenFetchFailed {
                message: "network".into(),
            })
        }
    }

    fn test_app(name: &str) -> FirebaseApp {
        FirebaseApp::new(
            FirebaseOptions::default(),
            FirebaseAppConfig::new(name.to_owned(), false),
            ComponentContainer::new(name.to_owned()),
        )
    }

    #[test]
    fn returns_token_string() {
        let provider = Arc::new(StaticTokenProvider {
            token: "app-check-123".into(),
        });
        let options = AppCheckOptions::new(provider);
        let app_check = initialize_app_check(Some(test_app("app-check-ok")), options).unwrap();
        let internal = FirebaseAppCheckInternal::new(app_check);
        let provider = AppCheckTokenProvider::new(internal);

        let token = provider.get_token().unwrap();
        assert_eq!(token.as_deref(), Some("app-check-123"));
    }

    #[test]
    fn propagates_errors() {
        let provider = Arc::new(ErrorProvider);
        let options = AppCheckOptions::new(provider);
        let app_check = initialize_app_check(Some(test_app("app-check-err")), options).unwrap();
        let internal = FirebaseAppCheckInternal::new(app_check);
        let provider = AppCheckTokenProvider::new(internal);

        let error = provider.get_token().unwrap_err();
        assert_eq!(
            error.code,
            crate::firestore::error::FirestoreErrorCode::Unavailable
        );
    }
}
