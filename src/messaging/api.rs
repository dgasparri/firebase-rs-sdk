use std::sync::{Arc, LazyLock};

use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

use crate::app;
use crate::app::FirebaseApp;
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType};
use crate::messaging::constants::MESSAGING_COMPONENT_NAME;
use crate::messaging::error::{
    internal_error, invalid_argument, token_deletion_failed, MessagingResult,
};
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use crate::messaging::token_store::{self, InstallationInfo, SubscriptionInfo, TokenRecord};
#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
use crate::messaging::token_store::{self, InstallationInfo, TokenRecord};

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use crate::messaging::constants::DEFAULT_VAPID_KEY;
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use crate::messaging::error::{available_in_window, permission_blocked, unsupported_browser};
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use wasm_bindgen::JsValue;
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use wasm_bindgen_futures::JsFuture;

#[derive(Clone, Debug)]
pub struct Messaging {
    inner: Arc<MessagingInner>,
}

#[derive(Debug)]
struct MessagingInner {
    app: FirebaseApp,
}

/// Notification permission states as exposed by the Web Notifications API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionState {
    /// The user has not decided whether to allow notifications.
    Default,
    /// The user granted notification permissions.
    Granted,
    /// The user denied notification permissions.
    Denied,
}

impl Messaging {
    fn new(app: FirebaseApp) -> Self {
        Self {
            inner: Arc::new(MessagingInner { app }),
        }
    }

    pub fn app(&self) -> &FirebaseApp {
        &self.inner.app
    }

    /// Requests browser notification permission.
    ///
    /// Port of the permission flow triggered by
    /// `packages/messaging/src/api/getToken.ts` in the Firebase JS SDK.
    pub async fn request_permission(&self) -> MessagingResult<PermissionState> {
        request_permission_impl().await
    }

    pub async fn get_token(&self, vapid_key: Option<&str>) -> MessagingResult<String> {
        get_token_impl(self, vapid_key).await
    }

    pub async fn delete_token(&self) -> MessagingResult<bool> {
        delete_token_impl(self).await
    }
}

fn generate_token() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .map(char::from)
        .take(32)
        .collect()
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
async fn request_permission_impl() -> MessagingResult<PermissionState> {
    use crate::messaging::support::is_supported;

    let window = web_sys::window()
        .ok_or_else(|| available_in_window("request_permission must run in a Window context"))?;

    // Access navigator to match the JS guard (throws if navigator is missing).
    let _navigator = window.navigator();

    if !is_supported() {
        return Err(unsupported_browser(
            "This browser does not expose the APIs required for Firebase Messaging.",
        ));
    }

    let current = web_sys::Notification::permission();
    match as_permission_state(&current) {
        PermissionState::Granted => return Ok(PermissionState::Granted),
        PermissionState::Denied => {
            return Err(permission_blocked(
                "Notification permission was previously blocked by the user.",
            ))
        }
        PermissionState::Default => {}
    }

    let promise = web_sys::Notification::request_permission()
        .map_err(|err| internal_error(format_js_error("requestPermission", err)))?;
    let result = JsFuture::from(promise)
        .await
        .map_err(|err| internal_error(format_js_error("requestPermission", err)))?;

    let status = result
        .as_string()
        .unwrap_or_else(|| web_sys::Notification::permission());

    match as_permission_state(&status) {
        PermissionState::Granted => Ok(PermissionState::Granted),
        PermissionState::Denied | PermissionState::Default => Err(permission_blocked(
            "Notification permission not granted by the user.",
        )),
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
fn as_permission_state(value: &str) -> PermissionState {
    match value {
        "granted" => PermissionState::Granted,
        "denied" => PermissionState::Denied,
        _ => PermissionState::Default,
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
fn format_js_error(operation: &str, err: JsValue) -> String {
    let detail = err.as_string().unwrap_or_else(|| format!("{:?}", err));
    format!("Notification.{operation} failed: {detail}")
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
async fn request_permission_impl() -> MessagingResult<PermissionState> {
    Ok(PermissionState::Granted)
}

static MESSAGING_COMPONENT: LazyLock<()> = LazyLock::new(|| {
    let component = Component::new(
        MESSAGING_COMPONENT_NAME,
        Arc::new(messaging_factory),
        ComponentType::Public,
    )
    .with_instantiation_mode(InstantiationMode::Lazy);
    let _ = app::registry::register_component(component);
});

fn messaging_factory(
    container: &crate::component::ComponentContainer,
    _options: InstanceFactoryOptions,
) -> Result<DynService, ComponentError> {
    let app = container.root_service::<FirebaseApp>().ok_or_else(|| {
        ComponentError::InitializationFailed {
            name: MESSAGING_COMPONENT_NAME.to_string(),
            reason: "Firebase app not attached to component container".to_string(),
        }
    })?;
    let messaging = Messaging::new((*app).clone());
    Ok(Arc::new(messaging) as DynService)
}

fn ensure_registered() {
    LazyLock::force(&MESSAGING_COMPONENT);
}

pub fn register_messaging_component() {
    ensure_registered();
}

pub fn get_messaging(app: Option<FirebaseApp>) -> MessagingResult<Arc<Messaging>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => crate::app::api::get_app(None).map_err(|err| internal_error(err.to_string()))?,
    };

    let provider = app::registry::get_provider(&app, MESSAGING_COMPONENT_NAME);
    if let Some(messaging) = provider.get_immediate::<Messaging>() {
        Ok(messaging)
    } else {
        provider
            .initialize::<Messaging>(serde_json::Value::Null, None)
            .map_err(|err| internal_error(err.to_string()))
    }
}

async fn get_token_impl(messaging: &Messaging, vapid_key: Option<&str>) -> MessagingResult<String> {
    #[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
    {
        get_token_wasm(messaging, vapid_key).await
    }

    #[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
    {
        get_token_native(messaging, vapid_key)
    }
}

fn app_store_key(messaging: &Messaging) -> String {
    messaging.inner.app.name().to_string()
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
fn get_token_native(messaging: &Messaging, vapid_key: Option<&str>) -> MessagingResult<String> {
    if let Some(key) = vapid_key {
        if key.trim().is_empty() {
            return Err(invalid_argument("VAPID key must not be empty"));
        }
    }

    let store_key = app_store_key(messaging);
    if let Some(record) = token_store::read_token(&store_key)? {
        if !record.is_expired(current_timestamp_ms(), TOKEN_EXPIRATION_MS) {
            return Ok(record.token);
        }
    }

    let token = generate_token();
    let record = TokenRecord {
        token: token.clone(),
        create_time_ms: current_timestamp_ms(),
        subscription: None,
        installation: dummy_installation_info(),
    };
    token_store::write_token(&store_key, &record)?;
    Ok(token)
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
async fn delete_token_impl(messaging: &Messaging) -> MessagingResult<bool> {
    let store_key = app_store_key(messaging);
    if token_store::remove_token(&store_key)? {
        Ok(true)
    } else {
        Err(token_deletion_failed("No token stored for this app"))
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
async fn get_token_wasm(messaging: &Messaging, vapid_key: Option<&str>) -> MessagingResult<String> {
    use crate::messaging::subscription::PushSubscriptionManager;
    use crate::messaging::support::is_supported;
    use crate::messaging::sw_manager::ServiceWorkerManager;

    if !is_supported() {
        return Err(unsupported_browser(
            "This browser does not expose the APIs required for Firebase Messaging.",
        ));
    }

    let vapid_key = vapid_key
        .filter(|key| !key.trim().is_empty())
        .unwrap_or(DEFAULT_VAPID_KEY);

    let mut sw_manager = ServiceWorkerManager::new();
    let registration = sw_manager.register_default().await?;
    let scope = registration
        .as_web_sys()
        .scope()
        .unwrap_or_else(|| String::from("/"));

    let mut push_manager = PushSubscriptionManager::new();
    let subscription = push_manager.subscribe(&registration, vapid_key).await?;
    let details = subscription.details()?;

    let subscription_info = SubscriptionInfo {
        vapid_key: vapid_key.to_string(),
        scope,
        endpoint: details.endpoint.clone(),
        auth: details.auth.clone(),
        p256dh: details.p256dh.clone(),
    };

    let store_key = app_store_key(messaging);
    let now_ms = current_timestamp_ms();
    if let Some(record) = token_store::read_token(&store_key).await? {
        if let Some(existing) = &record.subscription {
            if existing == &subscription_info && !record.is_expired(now_ms, TOKEN_EXPIRATION_MS) {
                return Ok(record.token);
            }
        }
    }

    let token = generate_token();
    let record = TokenRecord {
        token: token.clone(),
        create_time_ms: now_ms,
        subscription: Some(subscription_info),
        installation: dummy_installation_info(),
    };
    token_store::write_token(&store_key, &record).await?;
    Ok(token)
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
async fn delete_token_impl(messaging: &Messaging) -> MessagingResult<bool> {
    let store_key = app_store_key(messaging);
    if token_store::remove_token(&store_key).await? {
        Ok(true)
    } else {
        Err(token_deletion_failed("No token stored for this app"))
    }
}

fn current_timestamp_ms() -> u64 {
    #[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
    {
        js_sys::Date::now() as u64
    }

    #[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};

        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}
const TOKEN_EXPIRATION_MS: u64 = 7 * 24 * 60 * 60 * 1000;

fn dummy_installation_info() -> InstallationInfo {
    InstallationInfo {
        fid: "placeholder".to_string(),
        refresh_token: String::new(),
        auth_token: String::new(),
        auth_token_expiration_ms: u64::MAX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::api::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    fn unique_settings() -> FirebaseAppSettings {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!(
                "messaging-{}",
                COUNTER.fetch_add(1, Ordering::SeqCst)
            )),
            ..Default::default()
        }
    }

    #[test]
    fn token_is_stable_until_deleted() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let messaging = get_messaging(Some(app)).unwrap();
        let permission = block_on_ready(messaging.request_permission()).unwrap();
        assert_eq!(permission, PermissionState::Granted);
        let token1 = block_on_ready(messaging.get_token(None)).unwrap();
        let token2 = block_on_ready(messaging.get_token(None)).unwrap();
        assert_eq!(token1, token2);
        block_on_ready(messaging.delete_token()).unwrap();
        let token3 = block_on_ready(messaging.get_token(None)).unwrap();
        assert_ne!(token1, token3);
    }

    #[test]
    fn get_token_with_empty_vapid_key_returns_error() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let messaging = get_messaging(Some(app)).unwrap();
        let err = block_on_ready(messaging.get_token(Some(" "))).unwrap_err();
        assert_eq!(err.code_str(), "messaging/invalid-argument");
    }

    #[test]
    fn delete_token_without_existing_token_returns_error() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let messaging = get_messaging(Some(app)).unwrap();
        let err = block_on_ready(messaging.delete_token()).unwrap_err();
        assert_eq!(err.code_str(), "messaging/token-deletion-failed");
    }

    #[test]
    fn token_persists_across_messaging_instances() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let messaging = get_messaging(Some(app.clone())).unwrap();
        let token1 = block_on_ready(messaging.get_token(None)).unwrap();

        // Re-fetch messaging for the same app and validate the stored token is reused.
        let messaging_again = get_messaging(Some(app)).unwrap();
        let token2 = block_on_ready(messaging_again.get_token(None)).unwrap();
        assert_eq!(token1, token2);

        block_on_ready(messaging_again.delete_token()).unwrap();
    }

    fn block_on_ready<F: Future>(future: F) -> F::Output {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let mut future = future;
        // SAFETY: the future is dropped before this function returns and never moved while pinned.
        let mut pinned = unsafe { Pin::new_unchecked(&mut future) };
        match Future::poll(pinned.as_mut(), &mut cx) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("future unexpectedly pending"),
        }
    }

    fn noop_waker() -> Waker {
        unsafe { Waker::from_raw(noop_raw_waker()) }
    }

    fn noop_raw_waker() -> RawWaker {
        RawWaker::new(std::ptr::null(), &NOOP_RAW_WAKER_VTABLE)
    }

    unsafe fn noop_raw_waker_clone(_: *const ()) -> RawWaker {
        noop_raw_waker()
    }

    unsafe fn noop_raw_waker_wake(_: *const ()) {}

    unsafe fn noop_raw_waker_wake_by_ref(_: *const ()) {}

    unsafe fn noop_raw_waker_drop(_: *const ()) {}

    static NOOP_RAW_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
        noop_raw_waker_clone,
        noop_raw_waker_wake,
        noop_raw_waker_wake_by_ref,
        noop_raw_waker_drop,
    );
}
