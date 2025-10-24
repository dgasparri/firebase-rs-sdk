use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;

use crate::app::FirebaseApp;
use crate::platform::token::{AsyncTokenProvider, TokenError};
use crate::util::{PartialObserver, Unsubscribe};

use super::errors::{AppCheckError, AppCheckResult};

pub const APP_CHECK_COMPONENT_NAME: &str = "appCheck";
pub const APP_CHECK_INTERNAL_COMPONENT_NAME: &str = "app-check-internal";

#[derive(Clone, Debug)]
pub struct AppCheckToken {
    pub token: String,
    pub expire_time: SystemTime,
}

impl AppCheckToken {
    pub fn is_expired(&self) -> bool {
        SystemTime::now() >= self.expire_time
    }

    pub fn with_ttl(token: impl Into<String>, ttl: Duration) -> AppCheckResult<Self> {
        let expire_time = SystemTime::now().checked_add(ttl).ok_or_else(|| {
            AppCheckError::Internal("failed to compute token expiration".to_string())
        })?;
        Ok(Self {
            token: token.into(),
            expire_time,
        })
    }
}

#[derive(Clone, Debug)]
pub struct AppCheckTokenResult {
    pub token: String,
    pub error: Option<AppCheckError>,
    pub internal_error: Option<AppCheckError>,
}

impl AppCheckTokenResult {
    pub fn from_token(token: AppCheckToken) -> Self {
        Self {
            token: token.token,
            error: None,
            internal_error: None,
        }
    }

    pub fn from_error(error: AppCheckError) -> Self {
        Self {
            token: String::new(),
            error: Some(error),
            internal_error: None,
        }
    }

    pub fn from_internal_error(error: AppCheckError) -> Self {
        Self {
            token: String::new(),
            error: None,
            internal_error: Some(error),
        }
    }
}

pub type AppCheckTokenListener = Arc<dyn Fn(&AppCheckTokenResult) + Send + Sync + 'static>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListenerType {
    Internal,
    External,
}

#[derive(Clone)]
pub struct ListenerHandle {
    pub(crate) app_name: Arc<str>,
    pub(crate) id: u64,
    pub(crate) remover: Arc<dyn Fn(u64) + Send + Sync + 'static>,
    pub(crate) unsubscribed: Arc<AtomicBool>,
}

impl ListenerHandle {
    pub fn unsubscribe(&self) {
        if self
            .unsubscribed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            (self.remover)(self.id);
        }
    }
}

impl Drop for ListenerHandle {
    fn drop(&mut self) {
        self.unsubscribe();
    }
}

static LISTENER_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct AppCheckOptions {
    pub provider: Arc<dyn AppCheckProvider>,
    pub is_token_auto_refresh_enabled: Option<bool>,
}

impl AppCheckOptions {
    pub fn new(provider: Arc<dyn AppCheckProvider>) -> Self {
        Self {
            provider,
            is_token_auto_refresh_enabled: None,
        }
    }

    pub fn with_auto_refresh(mut self, enabled: bool) -> Self {
        self.is_token_auto_refresh_enabled = Some(enabled);
        self
    }
}

pub trait AppCheckProvider: Send + Sync {
    fn initialize(&self, _app: &FirebaseApp) {}

    fn get_token(&self) -> AppCheckResult<AppCheckToken>;

    fn get_limited_use_token(&self) -> AppCheckResult<AppCheckToken> {
        self.get_token()
    }
}

#[derive(Clone)]
pub struct AppCheck {
    app: FirebaseApp,
    app_name: Arc<str>,
}

impl AppCheck {
    pub(crate) fn new(app: FirebaseApp) -> Self {
        let app_name: Arc<str> = Arc::from(app.name().to_owned().into_boxed_str());
        Self { app, app_name }
    }

    pub fn app(&self) -> &FirebaseApp {
        &self.app
    }

    pub(crate) fn app_name(&self) -> Arc<str> {
        self.app_name.clone()
    }

    pub fn set_token_auto_refresh_enabled(&self, enabled: bool) {
        crate::app_check::api::set_token_auto_refresh_enabled(self, enabled);
    }

    pub async fn get_token(&self, force_refresh: bool) -> AppCheckResult<AppCheckTokenResult> {
        crate::app_check::api::get_token(self, force_refresh).await
    }

    pub async fn get_limited_use_token(&self) -> AppCheckResult<AppCheckTokenResult> {
        crate::app_check::api::get_limited_use_token(self).await
    }

    pub fn on_token_changed_with_observer(
        &self,
        observer: PartialObserver<AppCheckTokenResult>,
    ) -> AppCheckResult<Unsubscribe> {
        use crate::app_check::api::add_token_listener;

        let next = observer.next.clone();
        let listener = Arc::new(move |result: &AppCheckTokenResult| {
            if let Some(callback) = &next {
                callback(result);
            }
        });

        let handle = add_token_listener(self, listener, ListenerType::External)?;
        Ok(Box::new(move || handle.unsubscribe()))
    }

    pub fn on_token_changed<F, E, C>(
        &self,
        on_next: F,
        _on_error: Option<E>,
        _on_complete: Option<C>,
    ) -> AppCheckResult<Unsubscribe>
    where
        F: Fn(&AppCheckTokenResult) + Send + Sync + 'static,
        E: Fn(&dyn std::error::Error) + Send + Sync + 'static,
        C: Fn() + Send + Sync + 'static,
    {
        let observer = PartialObserver::new().with_next(on_next);
        self.on_token_changed_with_observer(observer)
    }
}

#[async_trait]
impl AsyncTokenProvider for Arc<AppCheck> {
    async fn get_token(&self, force_refresh: bool) -> Result<Option<String>, TokenError> {
        let result = AppCheck::get_token(self, force_refresh)
            .await
            .map_err(TokenError::from_error)?;

        if let Some(err) = result.error.or(result.internal_error) {
            return Err(TokenError::from_error(err));
        }

        if result.token.is_empty() {
            Ok(None)
        } else {
            Ok(Some(result.token))
        }
    }
}

pub type AppCheckInternalListener = Arc<dyn Fn(AppCheckTokenResult) + Send + Sync + 'static>;

#[derive(Clone)]
pub(crate) struct TokenListenerEntry {
    pub id: u64,
    pub listener: AppCheckTokenListener,
    _listener_type: ListenerType,
}

impl TokenListenerEntry {
    pub fn new(listener: AppCheckTokenListener, listener_type: ListenerType) -> Self {
        let id = LISTENER_ID.fetch_add(1, Ordering::SeqCst);
        Self {
            id,
            listener,
            _listener_type: listener_type,
        }
    }
}

#[derive(Clone)]
pub(crate) struct AppCheckState {
    pub activated: bool,
    pub provider: Option<Arc<dyn AppCheckProvider>>,
    pub token: Option<AppCheckToken>,
    pub is_token_auto_refresh_enabled: bool,
    pub observers: Vec<TokenListenerEntry>,
    #[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
    pub broadcast_handle: Option<crate::app_check::persistence::BroadcastSubscription>,
}

impl AppCheckState {
    pub fn new() -> Self {
        Self {
            activated: false,
            provider: None,
            token: None,
            is_token_auto_refresh_enabled: false,
            observers: Vec::new(),
            #[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
            broadcast_handle: None,
        }
    }
}

// SharedState helper removed in Rust port; state is tracked globally in state.rs.
