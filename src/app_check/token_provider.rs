// #![cfg(feature = "firestore")]

use std::sync::atomic::{AtomicBool, Ordering};

use crate::app_check::errors::AppCheckError;
use crate::app_check::FirebaseAppCheckInternal;
use crate::firestore::{
    internal_error, invalid_argument, unauthenticated, unavailable, FirestoreError, FirestoreResult,
};
use crate::firestore::{TokenProvider, TokenProviderArc};

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

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl TokenProvider for AppCheckTokenProvider {
    async fn get_token(&self) -> FirestoreResult<Option<String>> {
        let force_refresh = self.force_refresh.swap(false, Ordering::SeqCst);
        match self.app_check.get_token(force_refresh).await {
            Ok(result) => {
                if result.token.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(result.token))
                }
            }
            Err(err) => {
                if let Some(cached) = err.cached_token() {
                    if cached.token.is_empty() {
                        Ok(None)
                    } else {
                        Ok(Some(cached.token.clone()))
                    }
                } else {
                    Err(map_app_check_error(err.cause))
                }
            }
        }
    }

    fn invalidate_token(&self) {
        self.force_refresh.store(true, Ordering::SeqCst);
    }

    async fn heartbeat_header(&self) -> FirestoreResult<Option<String>> {
        self.app_check
            .heartbeat_header()
            .await
            .map_err(map_app_check_error)
    }
}

fn map_app_check_error(error: AppCheckError) -> FirestoreError {
    match error.clone() {
        AppCheckError::AlreadyInitialized { .. }
        | AppCheckError::UseBeforeActivation { .. }
        | AppCheckError::InvalidConfiguration { .. } => invalid_argument(error.to_string()),
        AppCheckError::TokenExpired => unauthenticated(error.to_string()),
        AppCheckError::Internal(message) => internal_error(message),
        AppCheckError::TokenFetchFailed { .. }
        | AppCheckError::ProviderError { .. }
        | AppCheckError::FetchNetworkError { .. }
        | AppCheckError::FetchParseError { .. }
        | AppCheckError::FetchStatusError { .. }
        | AppCheckError::RecaptchaError { .. }
        | AppCheckError::InitialThrottle { .. }
        | AppCheckError::Throttled { .. } => unavailable(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{FirebaseApp, FirebaseAppConfig, FirebaseOptions};
    use crate::app_check::api::{initialize_app_check, token_with_ttl};
    use crate::app_check::types::{
        box_app_check_future, AppCheckOptions, AppCheckProvider, AppCheckProviderFuture,
        AppCheckToken,
    };
    use crate::component::ComponentContainer;
    use std::sync::Arc;
    use std::time::Duration;

    #[derive(Clone)]
    struct StaticTokenProvider {
        token: String,
    }

    impl AppCheckProvider for StaticTokenProvider {
        fn get_token(
            &self,
        ) -> AppCheckProviderFuture<'_, crate::app_check::AppCheckResult<AppCheckToken>> {
            let token = self.token.clone();
            box_app_check_future(async move { token_with_ttl(token, Duration::from_secs(60)) })
        }
    }

    #[derive(Clone)]
    struct ErrorProvider;

    impl AppCheckProvider for ErrorProvider {
        fn get_token(
            &self,
        ) -> AppCheckProviderFuture<'_, crate::app_check::AppCheckResult<AppCheckToken>> {
            box_app_check_future(async move {
                Err(AppCheckError::TokenFetchFailed {
                    message: "network".into(),
                })
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

    #[tokio::test(flavor = "current_thread")]
    async fn returns_token_string() {
        let provider = Arc::new(StaticTokenProvider {
            token: "app-check-123".into(),
        });
        let options = AppCheckOptions::new(provider);
        let app_check = initialize_app_check(Some(test_app("app-check-ok")), options)
            .await
            .expect("initialize app check");
        let internal = FirebaseAppCheckInternal::new(app_check);
        let provider = AppCheckTokenProvider::new(internal);

        let token = provider.get_token().await.unwrap();
        assert_eq!(token.as_deref(), Some("app-check-123"));

        let heartbeat = provider.heartbeat_header().await.unwrap();
        assert!(heartbeat.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn propagates_errors() {
        let provider = Arc::new(ErrorProvider);
        let options = AppCheckOptions::new(provider);
        let app_check = initialize_app_check(Some(test_app("app-check-err")), options)
            .await
            .expect("initialize app check");
        let internal = FirebaseAppCheckInternal::new(app_check);
        let provider = AppCheckTokenProvider::new(internal);

        let error = provider.get_token().await.unwrap_err();
        assert_eq!(
            error.code,
            crate::firestore::FirestoreErrorCode::Unavailable
        );
    }
}
