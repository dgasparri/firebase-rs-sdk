use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use crate::app::{get_app, AppError, FirebaseApp, HeartbeatService, HeartbeatServiceImpl};
use crate::app::{get_provider, register_component};
use crate::component::types::{
    ComponentError, ComponentType, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentContainer};
use crate::platform::runtime;
use futures::FutureExt;
#[cfg(test)]
use std::sync::MutexGuard;

use super::errors::{AppCheckError, AppCheckResult};
use super::interop::FirebaseAppCheckInternal;
use super::logger::LOGGER;
use super::providers::{CustomProvider, ReCaptchaEnterpriseProvider, ReCaptchaV3Provider};
use super::refresher::Refresher;
use super::state;
use super::types::{
    AppCheck, AppCheckOptions, AppCheckProvider, AppCheckToken, AppCheckTokenError,
    AppCheckTokenErrorListener, AppCheckTokenListener, AppCheckTokenResult, ListenerHandle,
    ListenerType,
};

const TOKEN_REFRESH_OFFSET: Duration = Duration::from_secs(5 * 60);
const TOKEN_RETRY_MIN_WAIT: Duration = Duration::from_secs(30);
const TOKEN_RETRY_MAX_WAIT: Duration = Duration::from_secs(16 * 60);

struct AppCheckRegistryEntry {
    app_check: AppCheck,
    internal: FirebaseAppCheckInternal,
}

static REGISTRY: LazyLock<Mutex<HashMap<Arc<str>, AppCheckRegistryEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static APP_CHECK_COMPONENT: LazyLock<()> = LazyLock::new(|| {
    let component = Component::new(
        super::types::APP_CHECK_COMPONENT_NAME,
        Arc::new(app_check_factory),
        ComponentType::Public,
    )
    .with_instantiation_mode(InstantiationMode::Explicit);
    let _ = register_component(component);
});

static APP_CHECK_INTERNAL_COMPONENT: LazyLock<()> = LazyLock::new(|| {
    let component = Component::new(
        super::types::APP_CHECK_INTERNAL_COMPONENT_NAME,
        Arc::new(app_check_internal_factory),
        ComponentType::Private,
    )
    .with_instantiation_mode(InstantiationMode::Explicit);
    let _ = register_component(component);
});

fn ensure_components_registered() {
    LazyLock::force(&APP_CHECK_COMPONENT);
    LazyLock::force(&APP_CHECK_INTERNAL_COMPONENT);
}

fn app_check_factory(
    container: &ComponentContainer,
    _options: InstanceFactoryOptions,
) -> Result<DynService, ComponentError> {
    let app = container.root_service::<FirebaseApp>().ok_or_else(|| {
        ComponentError::InitializationFailed {
            name: super::types::APP_CHECK_COMPONENT_NAME.to_string(),
            reason: "Firebase app not attached to component container".to_string(),
        }
    })?;
    let app_name: Arc<str> = Arc::from(app.name().to_owned());
    let service = REGISTRY
        .lock()
        .unwrap()
        .get(&app_name)
        .map(|entry| entry.app_check.clone())
        .ok_or_else(|| ComponentError::InitializationFailed {
            name: super::types::APP_CHECK_COMPONENT_NAME.to_string(),
            reason: "App Check has not been initialized".to_string(),
        })?;
    Ok(Arc::new(service) as DynService)
}

fn app_check_internal_factory(
    container: &ComponentContainer,
    _options: InstanceFactoryOptions,
) -> Result<DynService, ComponentError> {
    let app = container.root_service::<FirebaseApp>().ok_or_else(|| {
        ComponentError::InitializationFailed {
            name: super::types::APP_CHECK_INTERNAL_COMPONENT_NAME.to_string(),
            reason: "Firebase app not attached to component container".to_string(),
        }
    })?;
    let app_name: Arc<str> = Arc::from(app.name().to_owned());
    let internal = REGISTRY
        .lock()
        .unwrap()
        .get(&app_name)
        .map(|entry| entry.internal.clone())
        .ok_or_else(|| ComponentError::InitializationFailed {
            name: super::types::APP_CHECK_INTERNAL_COMPONENT_NAME.to_string(),
            reason: "App Check has not been initialized".to_string(),
        })?;
    Ok(Arc::new(internal) as DynService)
}

/// Registers Firebase App Check for the given app using the supplied options.
///
/// Mirrors `initializeAppCheck` from the JS SDK (`packages/app-check/src/api.ts`).
/// Components are registered with the Firebase container, cached tokens are
/// restored when persistence is available, and the proactive refresh policy is
/// configured based on the provided options.
pub async fn initialize_app_check(
    app: Option<FirebaseApp>,
    options: AppCheckOptions,
) -> AppCheckResult<AppCheck> {
    ensure_components_registered();

    let app = if let Some(app) = app {
        app
    } else {
        match get_app(None).await {
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

    let provider = options.provider.clone();
    let requested_auto_refresh = options
        .is_token_auto_refresh_enabled
        .unwrap_or_else(|| app.automatic_data_collection_enabled());
    let final_auto_refresh = if requested_auto_refresh && !app.automatic_data_collection_enabled() {
        if matches!(options.is_token_auto_refresh_enabled, Some(true)) {
            LOGGER.warn(
                "`isTokenAutoRefreshEnabled` is true but `automaticDataCollectionEnabled` was set to false during `initialize_app()`. This blocks automatic token refresh.",
            );
        }
        false
    } else {
        requested_auto_refresh
    };

    let app_name: Arc<str> = Arc::from(app.name().to_owned());

    let heartbeat = get_provider(&app, "heartbeat")
        .get_immediate::<HeartbeatServiceImpl>()
        .map(|service| -> Arc<dyn HeartbeatService> { service });

    if let Some(service) = &heartbeat {
        let app_name = app.name().to_owned();
        let service_clone = service.clone();
        runtime::spawn_detached(async move {
            if let Err(err) = service_clone.trigger_heartbeat().await {
                LOGGER.debug(format!(
                    "Failed to trigger heartbeat for app {}: {}",
                    app_name, err
                ));
            }
        });
    }

    if let Some(entry) = REGISTRY.lock().unwrap().get(&app_name) {
        if !state::is_activated(&entry.app_check) {
            state::ensure_activation(&entry.app_check, provider.clone(), final_auto_refresh)?;
            provider.initialize(&app);
            if final_auto_refresh {
                maybe_start_refresher(&entry.app_check);
            }
            return Ok(entry.app_check.clone());
        }
        if let Some(current) = state::provider(&entry.app_check) {
            if !Arc::ptr_eq(&current, &provider) {
                return Err(AppCheckError::AlreadyInitialized {
                    app_name: app.name().to_owned(),
                });
            }
        }
        if state::auto_refresh_enabled(&entry.app_check) != final_auto_refresh {
            return Err(AppCheckError::AlreadyInitialized {
                app_name: app.name().to_owned(),
            });
        }
        return Ok(entry.app_check.clone());
    }

    let app_check = AppCheck::new(app.clone(), heartbeat.clone());
    state::ensure_activation(&app_check, provider.clone(), final_auto_refresh)?;
    provider.initialize(&app);

    let internal = FirebaseAppCheckInternal::new(app_check.clone());
    REGISTRY.lock().unwrap().insert(
        app_name.clone(),
        AppCheckRegistryEntry {
            app_check: app_check.clone(),
            internal,
        },
    );

    if final_auto_refresh {
        LOGGER.debug("App Check auto-refresh enabled");
        maybe_start_refresher(&app_check);
    }

    Ok(app_check)
}

/// Enables or disables proactive App Check token refresh.
///
/// Equivalent to `setTokenAutoRefreshEnabled` in the JS SDK. When enabled the
/// refresh scheduler follows the same midpoint + offset heuristics as
/// `proactive-refresh.ts`; disabling stops the background task and future calls
/// to [`get_token`] will fetch on demand.
pub fn set_token_auto_refresh_enabled(app_check: &AppCheck, enabled: bool) {
    state::set_auto_refresh(app_check, enabled);
    if enabled {
        LOGGER.debug("App Check auto-refresh toggled on");
        maybe_start_refresher(app_check);
    } else {
        state::clear_token_refresher(app_check);
    }
}

/// Returns the current App Check token, optionally forcing a refresh.
///
/// Mirrors `getToken` from `packages/app-check/src/internal-api.ts`. When a
/// cached token is still valid it is returned immediately; refresh failures are
/// reported through [`AppCheckTokenError`] so callers can distinguish fatal,
/// soft (cached-token) and throttled outcomes without relying on dummy tokens.
pub async fn get_token(
    app_check: &AppCheck,
    force_refresh: bool,
) -> Result<AppCheckTokenResult, AppCheckTokenError> {
    if !state::is_activated(app_check) {
        return Err(AppCheckTokenError::fatal(
            AppCheckError::UseBeforeActivation {
                app_name: app_check.app().name().to_owned(),
            },
        ));
    }

    let cached = state::current_token(app_check);
    if !force_refresh {
        if let Some(token) = cached.clone() {
            if !token.is_expired() {
                return Ok(AppCheckTokenResult::from_token(token));
            }
        }
    }

    let provider = state::provider(app_check).ok_or_else(|| {
        AppCheckTokenError::fatal(AppCheckError::UseBeforeActivation {
            app_name: app_check.app().name().to_owned(),
        })
    })?;

    match provider.get_token().await {
        Ok(token) => {
            state::store_token(app_check, token.clone());
            Ok(AppCheckTokenResult::from_token(token))
        }
        Err(err) => {
            let retry_after = throttle_retry_after(&err);
            let usable_cached = cached.filter(|token| !token.is_expired());

            let error = if let Some(token) = usable_cached {
                if let Some(wait) = retry_after {
                    AppCheckTokenError::throttled(err, wait, Some(token))
                } else {
                    AppCheckTokenError::soft(err, token)
                }
            } else if let Some(wait) = retry_after {
                AppCheckTokenError::throttled(err, wait, None)
            } else {
                AppCheckTokenError::fatal(err)
            };

            state::notify_token_error(app_check, &error);
            Err(error)
        }
    }
}

/// Fetches a limited-use App Check token from the underlying provider.
///
/// Equivalent to `getLimitedUseToken` in the JS SDK. Limited-use tokens always
/// hit the provider (or debug endpoint) and do not touch the cached state.
pub async fn get_limited_use_token(
    app_check: &AppCheck,
) -> Result<AppCheckTokenResult, AppCheckTokenError> {
    if !state::is_activated(app_check) {
        return Err(AppCheckTokenError::fatal(
            AppCheckError::UseBeforeActivation {
                app_name: app_check.app().name().to_owned(),
            },
        ));
    }

    let provider = state::provider(app_check).ok_or_else(|| {
        AppCheckTokenError::fatal(AppCheckError::UseBeforeActivation {
            app_name: app_check.app().name().to_owned(),
        })
    })?;

    match provider.get_limited_use_token().await {
        Ok(token) => Ok(AppCheckTokenResult::from_token(token)),
        Err(err) => {
            let error = if let Some(wait) = throttle_retry_after(&err) {
                AppCheckTokenError::throttled(err, wait, None)
            } else {
                AppCheckTokenError::fatal(err)
            };
            state::notify_token_error(app_check, &error);
            Err(error)
        }
    }
}

fn throttle_retry_after(error: &AppCheckError) -> Option<Duration> {
    match error {
        AppCheckError::InitialThrottle { retry_after, .. }
        | AppCheckError::Throttled { retry_after, .. } => Some(*retry_after),
        _ => None,
    }
}

/// Adds a listener that is notified whenever the cached token changes.
///
/// Behaviour matches the JS `addTokenListener` helper: listeners receive an
/// immediate callback when a valid token is already cached.
pub fn add_token_listener(
    app_check: &AppCheck,
    listener: AppCheckTokenListener,
    error_listener: Option<AppCheckTokenErrorListener>,
    listener_type: ListenerType,
) -> AppCheckResult<ListenerHandle> {
    if !state::is_activated(app_check) {
        return Err(AppCheckError::UseBeforeActivation {
            app_name: app_check.app().name().to_owned(),
        });
    }

    let handle = state::add_listener(app_check, listener.clone(), error_listener, listener_type);

    if let Some(token) = state::current_token(app_check) {
        listener(&AppCheckTokenResult::from_token(token));
    }

    if state::auto_refresh_enabled(app_check) {
        maybe_start_refresher(app_check);
    }

    Ok(handle)
}

/// Removes a listener previously registered via [`add_token_listener`].
pub fn remove_token_listener(handle: ListenerHandle) {
    handle.unsubscribe();
}

/// Builds a custom App Check provider from a synchronous callback.
///
/// Useful for emulators or bespoke attestation strategies. Mirrors the JS
/// `CustomProvider` helper (`packages/app-check/src/providers.ts`).
pub fn custom_provider<F>(callback: F) -> Arc<dyn AppCheckProvider>
where
    F: Fn() -> AppCheckResult<AppCheckToken> + Send + Sync + 'static,
{
    Arc::new(CustomProvider::new(callback))
}

/// Creates an App Check provider backed by reCAPTCHA v3 attestation.
///
/// The provider mirrors the JS SDK implementation: it renders an invisible reCAPTCHA
/// widget, exchanges each attested token with the App Check backend, and applies the
/// same throttling heuristics when the backend responds with rate-limited status
/// codes. The provider requires the `wasm-web` feature when targeting `wasm32` so the
/// browser runtime can bootstrap the reCAPTCHA scripts.
pub fn recaptcha_v3_provider(site_key: impl Into<String>) -> Arc<dyn AppCheckProvider> {
    Arc::new(ReCaptchaV3Provider::new(site_key.into()))
}

/// Creates an App Check provider backed by the reCAPTCHA Enterprise (score-based) API.
///
/// Behaviour matches the JS SDK variant: tokens are attested via the Enterprise
/// widget and exchanged with the App Check backend, and error throttling mirrors the
/// JavaScript semantics. As with the v3 provider, this requires the `wasm-web`
/// feature on `wasm32` builds to access the DOM and reCAPTCHA bootstrap scripts.
pub fn recaptcha_enterprise_provider(site_key: impl Into<String>) -> Arc<dyn AppCheckProvider> {
    Arc::new(ReCaptchaEnterpriseProvider::new(site_key.into()))
}

/// Convenience helper that constructs an [`AppCheckToken`] from a raw token string and TTL.
pub fn token_with_ttl(token: impl Into<String>, ttl: Duration) -> AppCheckResult<AppCheckToken> {
    AppCheckToken::with_ttl(token, ttl)
}

pub(super) fn on_token_stored(app_check: &AppCheck, _token: &AppCheckToken) {
    maybe_start_refresher(app_check);
}

fn ensure_refresher(app_check: &AppCheck) -> Refresher {
    state::ensure_token_refresher(app_check, || {
        let app_clone = app_check.clone();
        let operation = Arc::new(move || {
            let app = app_clone.clone();
            let future = async move {
                let has_token = state::peek_token(&app).is_some();
                let result = if has_token {
                    get_token(&app, true).await
                } else {
                    get_token(&app, false).await
                };

                match result {
                    Ok(_) => Ok(()),
                    Err(err) => Err(err.cause.clone()),
                }
            };
            #[cfg(not(target_arch = "wasm32"))]
            {
                future.boxed()
            }
            #[cfg(target_arch = "wasm32")]
            {
                future.boxed_local()
            }
        });

        let wait = Arc::new({
            let app = app_check.clone();
            move || compute_refresh_delay(&app)
        });

        Refresher::new(
            operation,
            Arc::new(|_| true),
            wait,
            TOKEN_RETRY_MIN_WAIT,
            TOKEN_RETRY_MAX_WAIT,
        )
    })
}

fn maybe_start_refresher(app_check: &AppCheck) {
    if !state::auto_refresh_enabled(app_check) {
        return;
    }
    let refresher = ensure_refresher(app_check);
    refresher.start();
}

fn compute_refresh_delay(app_check: &AppCheck) -> Duration {
    let Some(token) = state::peek_token(app_check) else {
        return Duration::ZERO;
    };

    let now = runtime::now();
    let ttl = token
        .expire_time
        .duration_since(token.issued_at)
        .unwrap_or(Duration::ZERO);
    let half_ttl_millis = ttl.as_millis() / 2;
    let half_ttl = Duration::from_millis(half_ttl_millis as u64);

    let mid_refresh = token
        .issued_at
        .checked_add(half_ttl)
        .and_then(|ts| ts.checked_add(TOKEN_REFRESH_OFFSET))
        .unwrap_or(token.expire_time);
    let latest = token
        .expire_time
        .checked_sub(TOKEN_REFRESH_OFFSET)
        .unwrap_or(token.expire_time);

    let target = if mid_refresh <= latest {
        mid_refresh
    } else {
        latest
    };

    match target.duration_since(now) {
        Ok(duration) => duration,
        Err(_) => Duration::ZERO,
    }
}

#[cfg(test)]
pub(crate) fn clear_registry() {
    REGISTRY.lock().unwrap().clear();
}

#[cfg(test)]
pub(crate) fn clear_state_for_tests() {
    state::clear_state();
}

#[cfg(test)]
pub(crate) fn test_guard() -> MutexGuard<'static, ()> {
    state::TEST_GUARD.lock().unwrap()
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::app::delete_app;
    use crate::app::{FirebaseApp, FirebaseAppConfig, FirebaseOptions};
    use crate::component::ComponentContainer;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::app_check::types::{box_app_check_future, AppCheckProviderFuture, TokenErrorKind};

    fn test_app(name: &str, automatic_data_collection_enabled: bool) -> FirebaseApp {
        FirebaseApp::new(
            FirebaseOptions::default(),
            FirebaseAppConfig::new(name.to_string(), automatic_data_collection_enabled),
            ComponentContainer::new(name.to_string()),
        )
    }

    #[derive(Clone)]
    struct FlakyProvider {
        calls: Arc<AtomicUsize>,
    }

    impl AppCheckProvider for FlakyProvider {
        fn get_token(&self) -> AppCheckProviderFuture<'_, AppCheckResult<AppCheckToken>> {
            let calls = Arc::clone(&self.calls);
            box_app_check_future(async move {
                let idx = calls.fetch_add(1, Ordering::SeqCst);
                if idx == 0 {
                    token_with_ttl("initial", Duration::from_secs(600))
                } else {
                    Err(AppCheckError::TokenFetchFailed {
                        message: "network".into(),
                    })
                }
            })
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cached_token_surfaces_internal_error() {
        let _guard = test_guard();
        clear_state_for_tests();
        clear_registry();
        let app = test_app("flaky", true);
        let provider = Arc::new(FlakyProvider {
            calls: Arc::new(AtomicUsize::new(0)),
        });
        let options = AppCheckOptions::new(provider);
        let app_check = initialize_app_check(Some(app.clone()), options)
            .await
            .expect("initialize app check");

        let first = get_token(&app_check, false).await.expect("first token");
        assert_eq!(first.token, "initial");

        let err = get_token(&app_check, true).await.expect_err("second token");
        match err.kind {
            TokenErrorKind::Soft { ref cached_token } => {
                assert_eq!(cached_token.token, "initial");
            }
            _ => panic!("expected soft error"),
        }

        delete_app(&app).await.expect("delete app");
        clear_state_for_tests();
        clear_registry();
    }

    #[derive(Clone)]
    struct StaticProvider;

    impl AppCheckProvider for StaticProvider {
        fn get_token(&self) -> AppCheckProviderFuture<'_, AppCheckResult<AppCheckToken>> {
            box_app_check_future(async { token_with_ttl("token", Duration::from_secs(600)) })
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn auto_refresh_can_be_toggled() {
        let _guard = test_guard();
        clear_state_for_tests();
        clear_registry();
        let app = test_app("auto", true);
        let provider = Arc::new(StaticProvider);
        let options = AppCheckOptions::new(provider).with_auto_refresh(true);
        let app_check = initialize_app_check(Some(app.clone()), options)
            .await
            .expect("initialize app check");

        let _ = get_token(&app_check, false).await.expect("token fetch");
        assert!(state::refresher_running(&app_check));

        set_token_auto_refresh_enabled(&app_check, false);
        assert!(!state::refresher_running(&app_check));

        delete_app(&app).await.expect("delete app");
        clear_state_for_tests();
        clear_registry();
    }
}
