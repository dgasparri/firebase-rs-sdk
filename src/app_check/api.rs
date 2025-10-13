use std::sync::Arc;
use std::time::Duration;

use crate::app::{get_app, AppError, FirebaseApp};

use super::errors::{AppCheckError, AppCheckResult};
use super::logger::LOGGER;
use super::providers::{CustomProvider, ReCaptchaEnterpriseProvider, ReCaptchaV3Provider};
use super::state;
use super::types::{
    AppCheck, AppCheckOptions, AppCheckProvider, AppCheckToken, AppCheckTokenListener,
    AppCheckTokenResult, ListenerHandle, ListenerType,
};

pub fn initialize_app_check(
    app: Option<FirebaseApp>,
    options: AppCheckOptions,
) -> AppCheckResult<AppCheck> {
    let app = if let Some(app) = app {
        app
    } else {
        match get_app(None) {
            Ok(app) => app,
            Err(AppError::NoApp { app_name }) => {
                return Err(AppCheckError::InvalidConfiguration {
                    message: format!("Firebase app '{app_name}' is not initialized"),
                });
            }
            Err(err) => {
                return Err(AppCheckError::Internal(err.to_string()));
            }
        }
    };

    let app_check = AppCheck::new(app.clone());

    let provider = options.provider.clone();
    let auto_refresh = options
        .is_token_auto_refresh_enabled
        .unwrap_or_else(|| app.automatic_data_collection_enabled());

    state::ensure_activation(&app_check, provider.clone(), auto_refresh)?;

    provider.initialize(&app);

    if auto_refresh {
        LOGGER.debug("App Check auto-refresh enabled");
    }

    Ok(app_check)
}

pub fn set_token_auto_refresh_enabled(app_check: &AppCheck, enabled: bool) {
    state::set_auto_refresh(app_check, enabled);
    if enabled {
        LOGGER.debug("App Check auto-refresh toggled on");
    }
}

pub fn get_token(app_check: &AppCheck, force_refresh: bool) -> AppCheckResult<AppCheckTokenResult> {
    if !state::is_activated(app_check) {
        return Err(AppCheckError::UseBeforeActivation {
            app_name: app_check.app().name().to_owned(),
        });
    }

    if !force_refresh {
        if let Some(token) = state::current_token(app_check) {
            if !token.is_expired() {
                return Ok(AppCheckTokenResult::from_token(token));
            }
        }
    }

    let provider =
        state::provider(app_check).ok_or_else(|| AppCheckError::UseBeforeActivation {
            app_name: app_check.app().name().to_owned(),
        })?;

    let token = provider.get_token()?;
    state::store_token(app_check, token.clone());
    Ok(AppCheckTokenResult::from_token(token))
}

pub fn get_limited_use_token(app_check: &AppCheck) -> AppCheckResult<AppCheckTokenResult> {
    if !state::is_activated(app_check) {
        return Err(AppCheckError::UseBeforeActivation {
            app_name: app_check.app().name().to_owned(),
        });
    }

    let provider =
        state::provider(app_check).ok_or_else(|| AppCheckError::UseBeforeActivation {
            app_name: app_check.app().name().to_owned(),
        })?;

    let token = provider.get_limited_use_token()?;
    Ok(AppCheckTokenResult::from_token(token))
}

pub fn add_token_listener(
    app_check: &AppCheck,
    listener: AppCheckTokenListener,
    listener_type: ListenerType,
) -> AppCheckResult<ListenerHandle> {
    if !state::is_activated(app_check) {
        return Err(AppCheckError::UseBeforeActivation {
            app_name: app_check.app().name().to_owned(),
        });
    }

    let handle = state::add_listener(app_check, listener.clone(), listener_type);

    if let Some(token) = state::current_token(app_check) {
        listener(&AppCheckTokenResult::from_token(token));
    }

    Ok(handle)
}

pub fn remove_token_listener(handle: ListenerHandle) {
    handle.unsubscribe();
}

// Helper constructors for simple providers.
pub fn custom_provider<F>(callback: F) -> Arc<dyn AppCheckProvider>
where
    F: Fn() -> AppCheckResult<AppCheckToken> + Send + Sync + 'static,
{
    Arc::new(CustomProvider::new(callback))
}

pub fn recaptcha_v3_provider(site_key: impl Into<String>) -> Arc<dyn AppCheckProvider> {
    Arc::new(ReCaptchaV3Provider::new(site_key.into()))
}

pub fn recaptcha_enterprise_provider(site_key: impl Into<String>) -> Arc<dyn AppCheckProvider> {
    Arc::new(ReCaptchaEnterpriseProvider::new(site_key.into()))
}

// Convenience helper to build AppCheck tokens for custom providers.
pub fn token_with_ttl(token: impl Into<String>, ttl: Duration) -> AppCheckResult<AppCheckToken> {
    AppCheckToken::with_ttl(token, ttl)
}
