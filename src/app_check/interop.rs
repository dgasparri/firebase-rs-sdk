use std::sync::{Arc, Mutex};

use crate::app_check::api;
use crate::app_check::errors::AppCheckResult;
#[cfg(target_arch = "wasm32")]
use crate::app_check::errors::AppCheckError;
use crate::app_check::types::{
    AppCheck, AppCheckInternalListener, AppCheckTokenResult, ListenerHandle, ListenerType,
};
use crate::firestore::remote::datastore::TokenProviderArc;

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

    pub fn get_token(&self, force_refresh: bool) -> AppCheckResult<AppCheckTokenResult> {
        api::get_token(&self.app_check, force_refresh)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn get_token_async(
        &self,
        force_refresh: bool,
    ) -> AppCheckResult<AppCheckTokenResult> {
        self.get_token(force_refresh)
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn get_token_async(
        &self,
        force_refresh: bool,
    ) -> AppCheckResult<AppCheckTokenResult> {
        let _ = force_refresh;
        Err(AppCheckError::Internal(
            "App Check token retrieval is not yet implemented for wasm targets".to_string(),
        ))
    }

    pub fn get_limited_use_token(&self) -> AppCheckResult<AppCheckTokenResult> {
        api::get_limited_use_token(&self.app_check)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn get_limited_use_token_async(&self) -> AppCheckResult<AppCheckTokenResult> {
        self.get_limited_use_token()
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn get_limited_use_token_async(&self) -> AppCheckResult<AppCheckTokenResult> {
        Err(AppCheckError::Internal(
            "Limited-use App Check tokens are not yet implemented for wasm targets".to_string(),
        ))
    }

    pub fn add_token_listener(&self, listener: AppCheckInternalListener) -> AppCheckResult<()> {
        let listeners = Arc::clone(&self.listeners);
        let listener_clone = Arc::clone(&listener);
        let bridge = Arc::new(move |result: &AppCheckTokenResult| {
            (*listener_clone)(result.clone());
        });

        let handle = api::add_token_listener(&self.app_check, bridge, ListenerType::Internal)?;
        listeners.lock().unwrap().push((listener, handle));
        Ok(())
    }

    pub fn remove_token_listener(&self, listener: &AppCheckInternalListener) {
        let mut listeners = self.listeners.lock().unwrap();
        if let Some(pos) = listeners
            .iter()
            .position(|(stored, _)| Arc::ptr_eq(stored, listener))
        {
            let (_, handle) = listeners.remove(pos);
            handle.unsubscribe();
        }
    }

    /// Exposes the internal App Check instance as a Firestore token provider.
    pub fn token_provider(&self) -> TokenProviderArc {
        app_check_token_provider_arc(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{FirebaseApp, FirebaseAppConfig, FirebaseOptions};
    use crate::app_check::api::{initialize_app_check, token_with_ttl};
    use crate::app_check::types::{AppCheckOptions, AppCheckProvider, AppCheckToken};
    use crate::component::ComponentContainer;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[derive(Clone)]
    struct TestProvider;

    impl AppCheckProvider for TestProvider {
        fn get_token(&self) -> AppCheckResult<AppCheckToken> {
            token_with_ttl("token", Duration::from_secs(60))
        }

        fn get_limited_use_token(&self) -> AppCheckResult<AppCheckToken> {
            token_with_ttl("limited", Duration::from_secs(60))
        }
    }

    fn test_app(name: &str) -> FirebaseApp {
        FirebaseApp::new(
            FirebaseOptions::default(),
            FirebaseAppConfig::new(name.to_string(), false),
            ComponentContainer::new(name.to_string()),
        )
    }

    fn setup_internal(name: &str) -> FirebaseAppCheckInternal {
        let app = test_app(name);
        let provider = Arc::new(TestProvider);
        let options = AppCheckOptions::new(provider);
        let app_check = initialize_app_check(Some(app), options).unwrap();
        FirebaseAppCheckInternal::new(app_check)
    }

    #[test]
    fn get_token_returns_value() {
        let internal = setup_internal("app-check-internal-test");
        let result = internal.get_token(false).unwrap();
        assert_eq!(result.token, "token");
    }

    #[test]
    fn listener_receives_updates_and_can_be_removed() {
        let internal = setup_internal("app-check-listener-test");
        let counter = Arc::new(AtomicUsize::new(0));
        let listener: AppCheckInternalListener = {
            let counter = counter.clone();
            Arc::new(move |result: AppCheckTokenResult| {
                assert_eq!(result.token, "token");
                counter.fetch_add(1, Ordering::SeqCst);
            })
        };

        // populate token cache
        internal.get_token(false).unwrap();
        internal.add_token_listener(listener.clone()).unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        internal.get_token(true).unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 2);

        internal.remove_token_listener(&listener);
        internal.get_token(true).unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }
}
