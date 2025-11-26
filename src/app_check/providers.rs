use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::app::{get_provider, FirebaseApp, HeartbeatService, HeartbeatServiceImpl};
use crate::util::calculate_backoff_millis;

use super::errors::{AppCheckError, AppCheckResult};
use super::types::{box_app_check_future, AppCheckProvider, AppCheckProviderFuture, AppCheckToken};
use crate::app_check::client::{
    exchange_token, get_exchange_recaptcha_enterprise_request, get_exchange_recaptcha_v3_request,
};
use crate::app_check::recaptcha::{self, RecaptchaFlow};

pub struct CustomProviderOptions {
    pub get_token: Arc<dyn Fn() -> AppCheckResult<AppCheckToken> + Send + Sync + 'static>,
    pub get_limited_use_token: Option<Arc<dyn Fn() -> AppCheckResult<AppCheckToken> + Send + Sync + 'static>>,
}

impl CustomProviderOptions {
    pub fn new<F>(callback: F) -> Self
    where
        F: Fn() -> AppCheckResult<AppCheckToken> + Send + Sync + 'static,
    {
        Self {
            get_token: Arc::new(callback),
            get_limited_use_token: None,
        }
    }

    pub fn with_limited_use<F>(mut self, callback: F) -> Self
    where
        F: Fn() -> AppCheckResult<AppCheckToken> + Send + Sync + 'static,
    {
        self.get_limited_use_token = Some(Arc::new(callback));
        self
    }
}

pub struct CustomProvider {
    options: CustomProviderOptions,
}

impl CustomProvider {
    pub fn new<F>(callback: F) -> Self
    where
        F: Fn() -> AppCheckResult<AppCheckToken> + Send + Sync + 'static,
    {
        Self {
            options: CustomProviderOptions::new(callback),
        }
    }

    pub fn from_options(options: CustomProviderOptions) -> Self {
        Self { options }
    }
}

impl AppCheckProvider for CustomProvider {
    fn get_token(&self) -> AppCheckProviderFuture<'_, AppCheckResult<AppCheckToken>> {
        let callback = Arc::clone(&self.options.get_token);
        box_app_check_future(async move { callback() })
    }

    fn get_limited_use_token(&self) -> AppCheckProviderFuture<'_, AppCheckResult<AppCheckToken>> {
        let callback = self
            .options
            .get_limited_use_token
            .as_ref()
            .cloned()
            .unwrap_or_else(|| Arc::clone(&self.options.get_token));
        box_app_check_future(async move { callback() })
    }
}

struct ProviderState {
    app: Option<FirebaseApp>,
    heartbeat: Option<Arc<dyn HeartbeatService>>,
    throttle: Option<ThrottleData>,
}

impl ProviderState {
    fn new() -> Self {
        Self {
            app: None,
            heartbeat: None,
            throttle: None,
        }
    }
}

#[derive(Clone, Debug)]
struct ThrottleData {
    backoff_count: u32,
    allow_requests_after: Instant,
    http_status: u16,
}

struct RecaptchaProviderCore {
    site_key: String,
    flow: RecaptchaFlow,
    state: Mutex<ProviderState>,
}

impl RecaptchaProviderCore {
    fn new(site_key: String, flow: RecaptchaFlow) -> Self {
        Self {
            site_key,
            flow,
            state: Mutex::new(ProviderState::new()),
        }
    }

    fn initialize(&self, app: &FirebaseApp) {
        let heartbeat = get_provider(app, "heartbeat")
            .get_immediate::<HeartbeatServiceImpl>()
            .map(|service| -> Arc<dyn HeartbeatService> { service });

        {
            let mut guard = self.state.lock().unwrap();
            guard.app = Some(app.clone());
            guard.heartbeat = heartbeat;
        }

        recaptcha::initialize(app, &self.site_key, self.flow);
    }

    async fn get_token(&self) -> AppCheckResult<AppCheckToken> {
        let (app, heartbeat) = {
            let mut guard = self.state.lock().unwrap();
            throw_if_throttled(&mut guard.throttle)?;
            let app = guard.app.clone().ok_or_else(|| AppCheckError::ProviderError {
                message: "ReCAPTCHA provider used before initialize()".into(),
            })?;
            let heartbeat = guard.heartbeat.clone();
            (app, heartbeat)
        };

        let recaptcha_token = recaptcha::get_token(&app).await?;
        if !recaptcha_token.succeeded {
            return Err(AppCheckError::RecaptchaError {
                message: Some("reCAPTCHA challenge failed".into()),
            });
        }

        let request = match self.flow {
            RecaptchaFlow::V3 => get_exchange_recaptcha_v3_request(&app, recaptcha_token.token.clone())?,
            RecaptchaFlow::Enterprise => {
                get_exchange_recaptcha_enterprise_request(&app, recaptcha_token.token.clone())?
            }
        };

        match exchange_token(request, heartbeat).await {
            Ok(token) => {
                let mut guard = self.state.lock().unwrap();
                guard.throttle = None;
                Ok(token)
            }
            Err(AppCheckError::FetchStatusError { http_status }) => {
                let mut guard = self.state.lock().unwrap();
                let previous = guard.throttle.take();
                let throttle = set_backoff(http_status, previous);
                let retry_after = throttle
                    .allow_requests_after
                    .checked_duration_since(Instant::now())
                    .unwrap_or_else(|| Duration::from_millis(0));
                guard.throttle = Some(throttle);
                Err(AppCheckError::InitialThrottle {
                    http_status,
                    retry_after,
                })
            }
            Err(err) => Err(err),
        }
    }
}

pub struct ReCaptchaV3Provider {
    core: RecaptchaProviderCore,
}

impl ReCaptchaV3Provider {
    pub fn new(site_key: String) -> Self {
        Self {
            core: RecaptchaProviderCore::new(site_key, RecaptchaFlow::V3),
        }
    }
}

impl AppCheckProvider for ReCaptchaV3Provider {
    fn initialize(&self, app: &FirebaseApp) {
        self.core.initialize(app);
    }

    fn get_token(&self) -> AppCheckProviderFuture<'_, AppCheckResult<AppCheckToken>> {
        box_app_check_future(async move { self.core.get_token().await })
    }
}

pub struct ReCaptchaEnterpriseProvider {
    core: RecaptchaProviderCore,
}

impl ReCaptchaEnterpriseProvider {
    pub fn new(site_key: String) -> Self {
        Self {
            core: RecaptchaProviderCore::new(site_key, RecaptchaFlow::Enterprise),
        }
    }
}

impl AppCheckProvider for ReCaptchaEnterpriseProvider {
    fn initialize(&self, app: &FirebaseApp) {
        self.core.initialize(app);
    }

    fn get_token(&self) -> AppCheckProviderFuture<'_, AppCheckResult<AppCheckToken>> {
        box_app_check_future(async move { self.core.get_token().await })
    }
}

const ONE_DAY: Duration = Duration::from_secs(24 * 60 * 60);

fn set_backoff(http_status: u16, previous: Option<ThrottleData>) -> ThrottleData {
    let now = Instant::now();
    if http_status == 403 || http_status == 404 {
        ThrottleData {
            backoff_count: 1,
            allow_requests_after: now + ONE_DAY,
            http_status,
        }
    } else {
        let backoff_count = previous.map(|data| data.backoff_count).unwrap_or(0);
        let wait_millis = calculate_backoff_millis(backoff_count);
        ThrottleData {
            backoff_count: backoff_count + 1,
            allow_requests_after: now + Duration::from_millis(wait_millis),
            http_status,
        }
    }
}

fn throw_if_throttled(throttle: &mut Option<ThrottleData>) -> AppCheckResult<()> {
    if let Some(data) = throttle {
        if let Some(retry_after) = data.allow_requests_after.checked_duration_since(Instant::now()) {
            if !retry_after.is_zero() {
                return Err(AppCheckError::Throttled {
                    http_status: data.http_status,
                    retry_after,
                });
            }
        } else {
            *throttle = None;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{FirebaseApp, FirebaseAppConfig, FirebaseOptions};
    use crate::app_check::client;
    use crate::app_check::recaptcha::{self, RecaptchaDriver, RecaptchaFlow, RecaptchaTokenDetails};
    use crate::component::ComponentContainer;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{LazyLock, Mutex as StdMutex, MutexGuard};

    static TEST_LOCK: LazyLock<StdMutex<()>> = LazyLock::new(|| StdMutex::new(()));

    struct OverridesGuard {
        _lock: MutexGuard<'static, ()>,
    }

    impl OverridesGuard {
        fn new() -> Self {
            let lock = TEST_LOCK.lock().unwrap();
            recaptcha::clear_driver_override();
            client::clear_exchange_override();
            Self { _lock: lock }
        }
    }

    impl Drop for OverridesGuard {
        fn drop(&mut self) {
            recaptcha::clear_driver_override();
            client::clear_exchange_override();
        }
    }

    #[derive(Clone)]
    struct StubRecaptchaDriver {
        token: String,
        succeeded: bool,
    }

    impl StubRecaptchaDriver {
        fn new(token: impl Into<String>, succeeded: bool) -> Self {
            Self {
                token: token.into(),
                succeeded,
            }
        }
    }

    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    impl RecaptchaDriver for StubRecaptchaDriver {
        fn initialize(&self, _app: &FirebaseApp, _site_key: &str, _flow: RecaptchaFlow) {}

        async fn get_token(&self, _app: &FirebaseApp) -> AppCheckResult<RecaptchaTokenDetails> {
            Ok(RecaptchaTokenDetails {
                token: self.token.clone(),
                succeeded: self.succeeded,
            })
        }
    }

    fn test_app() -> FirebaseApp {
        FirebaseApp::new(
            FirebaseOptions {
                api_key: Some("test-key".into()),
                app_id: Some("test-app".into()),
                project_id: Some("test-project".into()),
                ..Default::default()
            },
            FirebaseAppConfig::new("recaptcha-provider-test", false),
            ComponentContainer::new("recaptcha-provider-test"),
        )
    }

    #[tokio::test(flavor = "current_thread")]
    async fn v3_provider_exchanges_token() {
        let _guard = OverridesGuard::new();
        recaptcha::set_driver_override(Arc::new(StubRecaptchaDriver::new("captcha-token", true)));
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_clone = calls.clone();
        client::set_exchange_override(move |request, _| {
            let calls = calls_clone.clone();
            assert!(request.url.contains("exchangeRecaptchaV3Token"));
            assert_eq!(request.body["recaptcha_v3_token"], "captcha-token");
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                AppCheckToken::with_ttl("app-check-token", Duration::from_secs(60))
            }
        });

        let app = test_app();
        let provider = ReCaptchaV3Provider::new("site-key".into());
        provider.initialize(&app);

        let token = provider.get_token().await.expect("token");
        assert_eq!(token.token, "app-check-token");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn v3_provider_throttles_on_403() {
        let _guard = OverridesGuard::new();
        recaptcha::set_driver_override(Arc::new(StubRecaptchaDriver::new("captcha", true)));

        let calls = Arc::new(AtomicUsize::new(0));
        let calls_clone = calls.clone();
        client::set_exchange_override(move |_, _| {
            let calls = calls_clone.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Err(AppCheckError::FetchStatusError { http_status: 403 })
            }
        });

        let app = test_app();
        let provider = ReCaptchaV3Provider::new("site-key".into());
        provider.initialize(&app);

        let err = provider.get_token().await.err().expect("error");
        match err {
            AppCheckError::InitialThrottle {
                http_status,
                retry_after,
            } => {
                assert_eq!(http_status, 403);
                assert!(retry_after.as_secs() >= (24 * 60 * 60 - 60));
            }
            other => panic!("unexpected error: {}", other),
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let err = provider.get_token().await.err().expect("throttled");
        match err {
            AppCheckError::Throttled {
                http_status,
                retry_after,
            } => {
                assert_eq!(http_status, 403);
                assert!(retry_after.as_secs() >= (24 * 60 * 60 - 60));
            }
            other => panic!("unexpected error: {}", other),
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn v3_provider_recovers_after_backoff() {
        let _guard = OverridesGuard::new();
        recaptcha::set_driver_override(Arc::new(StubRecaptchaDriver::new("captcha", true)));

        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();
        client::set_exchange_override(move |_, _| {
            let attempts = attempts_clone.clone();
            async move {
                let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                match attempt {
                    0 | 1 => Err(AppCheckError::FetchStatusError { http_status: 503 }),
                    _ => AppCheckToken::with_ttl("token", Duration::from_secs(60)),
                }
            }
        });

        let app = test_app();
        let provider = ReCaptchaV3Provider::new("site-key".into());
        provider.initialize(&app);

        let err = provider.get_token().await.err().expect("initial throttle");
        assert!(matches!(err, AppCheckError::InitialThrottle { http_status: 503, .. }));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);

        let err = provider.get_token().await.err().expect("throttled");
        assert!(matches!(err, AppCheckError::Throttled { http_status: 503, .. }));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);

        {
            let mut guard = provider.core.state.lock().unwrap();
            if let Some(throttle) = guard.throttle.as_mut() {
                throttle.allow_requests_after = Instant::now();
            }
        }

        let err = provider.get_token().await.err().expect("second throttle");
        assert!(matches!(err, AppCheckError::InitialThrottle { http_status: 503, .. }));
        assert_eq!(attempts.load(Ordering::SeqCst), 2);

        {
            let mut guard = provider.core.state.lock().unwrap();
            if let Some(throttle) = guard.throttle.as_mut() {
                throttle.allow_requests_after = Instant::now();
            }
        }

        let token = provider.get_token().await.expect("token");
        assert_eq!(token.token, "token");
        assert!(attempts.load(Ordering::SeqCst) >= 3);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn recaptcha_failure_surfaces_error() {
        let _guard = OverridesGuard::new();
        recaptcha::set_driver_override(Arc::new(StubRecaptchaDriver::new("captcha", false)));

        let app = test_app();
        let provider = ReCaptchaV3Provider::new("site-key".into());
        provider.initialize(&app);

        let err = provider.get_token().await.err().expect("error");
        assert!(matches!(err, AppCheckError::RecaptchaError { .. }));
    }
}
