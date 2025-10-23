use std::fmt::{Debug, Formatter};
use std::sync::{Arc, Mutex};

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::FirebaseApp;
use crate::app_check::FirebaseAppCheckInternal;
use crate::auth::Auth;
use crate::component::provider::Provider;
use crate::messaging::Messaging;

/// Metadata that may be attached to callable Function requests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CallContext {
    pub auth_token: Option<String>,
    pub messaging_token: Option<String>,
    pub app_check_token: Option<String>,
}

pub struct ContextProvider {
    auth_provider: Provider,
    auth_internal_provider: Provider,
    messaging_provider: Provider,
    app_check_provider: Provider,
    cached_auth: Mutex<Option<Arc<Auth>>>,
    cached_messaging: Mutex<Option<Arc<Messaging>>>,
    cached_app_check: Mutex<Option<Arc<FirebaseAppCheckInternal>>>,
    overrides: Mutex<Option<CallContext>>,
}

impl Debug for ContextProvider {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let auth_cached = self.cached_auth.lock().unwrap().is_some();
        let messaging_cached = self.cached_messaging.lock().unwrap().is_some();
        let app_check_cached = self.cached_app_check.lock().unwrap().is_some();
        f.debug_struct("ContextProvider")
            .field("auth_cached", &auth_cached)
            .field("messaging_cached", &messaging_cached)
            .field("app_check_cached", &app_check_cached)
            .finish()
    }
}

impl ContextProvider {
    pub fn new(app: FirebaseApp) -> Self {
        let container = app.container();
        let auth_provider = container.get_provider("auth");
        let auth_internal_provider = container.get_provider("auth-internal");
        let messaging_provider = container.get_provider("messaging");
        let app_check_provider = container.get_provider("app-check-internal");

        Self {
            auth_provider,
            auth_internal_provider,
            messaging_provider,
            app_check_provider,
            cached_auth: Mutex::new(None),
            cached_messaging: Mutex::new(None),
            cached_app_check: Mutex::new(None),
            overrides: Mutex::new(None),
        }
    }

    pub async fn get_context_async(&self, limited_use_app_check_tokens: bool) -> CallContext {
        if let Some(overrides) = self.overrides.lock().unwrap().clone() {
            return overrides;
        }

        CallContext {
            auth_token: self.fetch_auth_token().await,
            messaging_token: self.fetch_messaging_token().await,
            app_check_token: self
                .fetch_app_check_token(limited_use_app_check_tokens)
                .await,
        }
    }

    async fn fetch_auth_token(&self) -> Option<String> {
        let auth = self.ensure_auth()?;
        match auth.get_token_async(false).await {
            Ok(Some(token)) if !token.is_empty() => Some(token),
            _ => None,
        }
    }

    async fn fetch_messaging_token(&self) -> Option<String> {
        #[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
        {
            use crate::messaging::token_store;

            const MESSAGING_TOKEN_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1000;

            let messaging = self.ensure_messaging()?;
            let store_key = messaging.app().name().to_string();
            if let Ok(Some(record)) = token_store::read_token_async(&store_key).await {
                let now_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|duration| duration.as_millis() as u64)
                    .unwrap_or(0);
                if !record.is_expired(now_ms, MESSAGING_TOKEN_TTL_MS) {
                    return Some(record.token);
                }
            }
        }

        #[allow(unreachable_code)]
        None
    }

    async fn fetch_app_check_token(&self, limited_use: bool) -> Option<String> {
        let app_check = self.ensure_app_check()?;
        let result = if limited_use {
            app_check.get_limited_use_token_async().await
        } else {
            app_check.get_token_async(false).await
        };

        match result {
            Ok(result) => {
                if result.error.is_some() || result.internal_error.is_some() {
                    return None;
                }
                if result.token.is_empty() {
                    None
                } else {
                    Some(result.token)
                }
            }
            Err(_) => None,
        }
    }

    fn ensure_auth(&self) -> Option<Arc<Auth>> {
        if let Some(cached) = self.cached_auth.lock().unwrap().clone() {
            return Some(cached);
        }

        let maybe_auth = self
            .auth_internal_provider
            .get_immediate::<Auth>()
            .or_else(|| self.auth_provider.get_immediate::<Auth>());

        if let Some(auth) = maybe_auth {
            *self.cached_auth.lock().unwrap() = Some(auth.clone());
            Some(auth)
        } else {
            None
        }
    }

    fn ensure_messaging(&self) -> Option<Arc<Messaging>> {
        if let Some(cached) = self.cached_messaging.lock().unwrap().clone() {
            return Some(cached);
        }

        if let Some(messaging) = self.messaging_provider.get_immediate::<Messaging>() {
            *self.cached_messaging.lock().unwrap() = Some(messaging.clone());
            Some(messaging)
        } else {
            None
        }
    }

    fn ensure_app_check(&self) -> Option<Arc<FirebaseAppCheckInternal>> {
        if let Some(cached) = self.cached_app_check.lock().unwrap().clone() {
            return Some(cached);
        }

        if let Some(app_check) = self
            .app_check_provider
            .get_immediate::<FirebaseAppCheckInternal>()
        {
            *self.cached_app_check.lock().unwrap() = Some(app_check.clone());
            Some(app_check)
        } else {
            None
        }
    }

    #[cfg(test)]
    pub fn set_overrides(&self, overrides: CallContext) {
        *self.overrides.lock().unwrap() = Some(overrides);
    }
}
