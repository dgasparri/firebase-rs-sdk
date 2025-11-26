use std::sync::{Arc, Mutex};

use crate::app_check::api;
use crate::app_check::errors::AppCheckResult;
use crate::app_check::types::{
    AppCheck, AppCheckInternalListener, AppCheckTokenError, AppCheckTokenResult, ListenerHandle, ListenerType,
};
//#[cfg(feature = "firestore")]
use crate::firestore::TokenProviderArc;

//#[cfg(feature = "firestore")]
use super::token_provider::app_check_token_provider_arc;

#[derive(Clone)]
pub struct FirebaseAppCheckInternal {
    app_check: AppCheck,
    listeners: Arc<Mutex<Vec<(AppCheckInternalListener, ListenerHandle)>>>,
}

impl FirebaseAppCheckInternal {
    pub fn new(app_check: AppCheck) -> Self {
        Self {
            app_check,
            listeners: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn app_check(&self) -> &AppCheck {
        &self.app_check
    }

    pub async fn get_token(&self, force_refresh: bool) -> Result<AppCheckTokenResult, AppCheckTokenError> {
        api::get_token(&self.app_check, force_refresh).await
    }

    pub async fn get_limited_use_token(&self) -> Result<AppCheckTokenResult, AppCheckTokenError> {
        api::get_limited_use_token(&self.app_check).await
    }

    pub async fn heartbeat_header(&self) -> AppCheckResult<Option<String>> {
        self.app_check.heartbeat_header().await
    }

    pub fn add_token_listener(&self, listener: AppCheckInternalListener) -> AppCheckResult<()> {
        let listeners = Arc::clone(&self.listeners);
        let listener_clone = Arc::clone(&listener);
        let bridge = Arc::new(move |result: &AppCheckTokenResult| {
            (*listener_clone)(result.clone());
        });

        let handle = api::add_token_listener(&self.app_check, bridge, None, ListenerType::Internal)?;
        listeners.lock().unwrap().push((listener, handle));
        Ok(())
    }

    pub fn remove_token_listener(&self, listener: &AppCheckInternalListener) {
        let mut listeners = self.listeners.lock().unwrap();
        if let Some(pos) = listeners.iter().position(|(stored, _)| Arc::ptr_eq(stored, listener)) {
            let (_, handle) = listeners.remove(pos);
            handle.unsubscribe();
        }
    }

    /// Exposes the internal App Check instance as a Firestore token provider.
    //#[cfg(feature = "firestore")]
    pub fn token_provider(&self) -> TokenProviderArc {
        app_check_token_provider_arc(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{FirebaseApp, FirebaseAppConfig, FirebaseOptions};
    use crate::app_check::api::{
        clear_registry, clear_state_for_tests, initialize_app_check, test_guard, token_with_ttl,
    };
    use crate::app_check::types::{
        box_app_check_future, AppCheckOptions, AppCheckProvider, AppCheckProviderFuture, AppCheckToken,
    };
    use crate::component::ComponentContainer;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[derive(Clone)]
    struct TestProvider;

    impl AppCheckProvider for TestProvider {
        fn get_token(&self) -> AppCheckProviderFuture<'_, AppCheckResult<AppCheckToken>> {
            box_app_check_future(async { token_with_ttl("token", Duration::from_secs(60)) })
        }

        fn get_limited_use_token(&self) -> AppCheckProviderFuture<'_, AppCheckResult<AppCheckToken>> {
            box_app_check_future(async { token_with_ttl("limited", Duration::from_secs(60)) })
        }
    }

    fn test_app(name: &str) -> FirebaseApp {
        FirebaseApp::new(
            FirebaseOptions::default(),
            FirebaseAppConfig::new(name.to_string(), false),
            ComponentContainer::new(name.to_string()),
        )
    }

    async fn setup_internal(name: &str) -> FirebaseAppCheckInternal {
        clear_state_for_tests();
        clear_registry();
        let app = test_app(name);
        let provider = Arc::new(TestProvider);
        let options = AppCheckOptions::new(provider);
        let app_check = initialize_app_check(Some(app), options)
            .await
            .expect("initialize app check");
        FirebaseAppCheckInternal::new(app_check)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn get_token_returns_value() {
        let _guard = test_guard();
        let internal = setup_internal("app-check-internal-test").await;
        let result = internal.get_token(false).await.unwrap();
        assert_eq!(result.token, "token");

        let heartbeat = internal.heartbeat_header().await.unwrap();
        assert!(heartbeat.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn listener_receives_updates_and_can_be_removed() {
        let _guard = test_guard();
        let internal = setup_internal("app-check-listener-test").await;
        let counter = Arc::new(AtomicUsize::new(0));
        let listener: AppCheckInternalListener = {
            let counter = counter.clone();
            Arc::new(move |result: AppCheckTokenResult| {
                assert_eq!(result.token, "token");
                counter.fetch_add(1, Ordering::SeqCst);
            })
        };

        // populate token cache
        internal.get_token(false).await.unwrap();
        internal.add_token_listener(listener.clone()).unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        internal.get_token(true).await.unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 2);

        internal.remove_token_listener(&listener);
        internal.get_token(true).await.unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }
}
