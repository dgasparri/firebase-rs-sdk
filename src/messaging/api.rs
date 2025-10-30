#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};
use std::sync::{Arc, LazyLock};

#[cfg(not(all(
    feature = "wasm-web",
    target_arch = "wasm32"
)))]
use rand::distributions::Alphanumeric;
#[cfg(not(all(
    feature = "wasm-web",
    target_arch = "wasm32"
)))]
use rand::{thread_rng, Rng};

use crate::app;
use crate::app::FirebaseApp;
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType};
#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
use crate::installations::extract_app_config;
#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
use crate::installations::{get_installations_internal, InstallationEntryData};
use crate::messaging::constants::MESSAGING_COMPONENT_NAME;
#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
use crate::messaging::error::invalid_argument;
use crate::messaging::error::{
    available_in_service_worker, available_in_window, internal_error, token_deletion_failed,
    MessagingResult,
};
#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
use crate::messaging::fcm_rest::{
    FcmClient, FcmRegistrationRequest, FcmSubscription, FcmUpdateRequest,
};
#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
use crate::messaging::token_store::{self, InstallationInfo, SubscriptionInfo, TokenRecord};
#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
use crate::messaging::token_store::{self, InstallationInfo, TokenRecord};
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use crate::messaging::types::MessagePayload;
use crate::messaging::types::{MessageHandler, Unsubscribe};
#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use web_sys::NotificationPermission;

#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
use crate::messaging::constants::DEFAULT_VAPID_KEY;
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use crate::messaging::error::{permission_blocked, unsupported_browser};
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use wasm_bindgen::{JsCast, JsValue};
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use wasm_bindgen_futures::JsFuture;

#[derive(Clone, Debug)]
pub struct Messaging {
    inner: Arc<MessagingInner>,
}

#[derive(Debug)]
struct MessagingInner {
    app: FirebaseApp,
    #[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
    on_message_handler: Mutex<Option<HandlerEntry>>,
    #[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
    on_background_message_handler: Mutex<Option<HandlerEntry>>,
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
#[derive(Clone)]
struct HandlerEntry {
    id: usize,
    #[allow(dead_code)]
    handler: MessageHandler,
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
impl std::fmt::Debug for HandlerEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandlerEntry")
            .field("id", &self.id)
            .finish()
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
static NEXT_ON_MESSAGE_ID: AtomicUsize = AtomicUsize::new(1);

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
static NEXT_ON_BACKGROUND_ID: AtomicUsize = AtomicUsize::new(1);

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
        let inner = MessagingInner {
            app,
            #[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
            on_message_handler: Mutex::new(None),
            #[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
            on_background_message_handler: Mutex::new(None),
        };
        Self {
            inner: Arc::new(inner),
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


    #[cfg_attr(not(test), allow(dead_code))]
    #[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
    #[allow(dead_code)]
    pub(crate) fn dispatch_on_message(&self, payload: MessagePayload) {
        let handler = {
            self.inner
                .on_message_handler
                .lock()
                .unwrap()
                .as_ref()
                .map(|entry| entry.handler.clone())
        };
        if let Some(handler) = handler {
            handler(payload);
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    #[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
    #[allow(dead_code)]
    pub(crate) fn dispatch_on_background_message(&self, payload: MessagePayload) {
        let handler = {
            self.inner
                .on_background_message_handler
                .lock()
                .unwrap()
                .as_ref()
                .map(|entry| entry.handler.clone())
        };
        if let Some(handler) = handler {
            handler(payload);
        }
    }
}

//#[cfg(not(all(
//    feature = "wasm-web",
//    target_arch = "wasm32",
//    feature = "experimental-indexed-db"
//)))]
#[cfg(not(target_arch = "wasm32"))]
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
    match permission_state_from_enum(current) {
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

    let status = result.as_string();
    let permission_state = status
        .as_deref()
        .map(permission_state_from_str)
        .unwrap_or_else(|| permission_state_from_enum(web_sys::Notification::permission()));

    match permission_state {
        PermissionState::Granted => Ok(PermissionState::Granted),
        PermissionState::Denied | PermissionState::Default => Err(permission_blocked(
            "Notification permission not granted by the user.",
        )),
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
fn permission_state_from_enum(value: NotificationPermission) -> PermissionState {
    match value {
        NotificationPermission::Granted => PermissionState::Granted,
        NotificationPermission::Denied => PermissionState::Denied,
        _ => PermissionState::Default,
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
fn permission_state_from_str(value: &str) -> PermissionState {
    match value {
        "granted" => PermissionState::Granted,
        "denied" => PermissionState::Denied,
        _ => PermissionState::Default,
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
fn format_js_error(operation: &str, err: JsValue) -> String {
    if let Some(message) = err.as_string() {
        format!("{operation} failed: {message}")
    } else if let Some(exception) = err.dyn_ref::<web_sys::DomException>() {
        format!(
            "{operation} failed: {}: {}",
            exception.name(),
            exception.message()
        )
    } else {
        format!("{operation} failed: {:?}", err)
    }
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

pub async fn get_messaging(app: Option<FirebaseApp>) -> MessagingResult<Arc<Messaging>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => crate::app::api::get_app(None)
            .await
            .map_err(|err| internal_error(err.to_string()))?,
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
        get_token_native(messaging, vapid_key).await
    }
}

#[allow(dead_code)]
fn app_store_key(messaging: &Messaging) -> String {
    messaging.inner.app.name().to_string()
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
pub fn on_message(messaging: &Messaging, handler: MessageHandler) -> MessagingResult<Unsubscribe> {
    if web_sys::window().is_none() {
        return Err(available_in_window(
            "on_message must be called in a Window context",
        ));
    }

    let id = NEXT_ON_MESSAGE_ID.fetch_add(1, Ordering::SeqCst);
    let messaging_clone = messaging.clone();
    {
        let mut guard = messaging_clone.inner.on_message_handler.lock().unwrap();
        *guard = Some(HandlerEntry { id, handler });
    }

    Ok(Box::new(move || {
        let mut guard = messaging_clone.inner.on_message_handler.lock().unwrap();
        if guard.as_ref().map(|entry| entry.id) == Some(id) {
            *guard = None;
        }
    }))
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
pub fn on_message(
    _messaging: &Messaging,
    _handler: MessageHandler,
) -> MessagingResult<Unsubscribe> {
    Err(available_in_window(
        "on_message must be called in a Window context (wasm target only)",
    ))
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
pub fn on_background_message(
    messaging: &Messaging,
    handler: MessageHandler,
) -> MessagingResult<Unsubscribe> {
    if web_sys::window().is_some() {
        return Err(available_in_service_worker(
            "on_background_message must be called in a Service Worker context",
        ));
    }

    let id = NEXT_ON_BACKGROUND_ID.fetch_add(1, Ordering::SeqCst);
    let messaging_clone = messaging.clone();
    {
        let mut guard = messaging_clone
            .inner
            .on_background_message_handler
            .lock()
            .unwrap();
        *guard = Some(HandlerEntry { id, handler });
    }

    Ok(Box::new(move || {
        let mut guard = messaging_clone
            .inner
            .on_background_message_handler
            .lock()
            .unwrap();
        if guard.as_ref().map(|entry| entry.id) == Some(id) {
            *guard = None;
        }
    }))
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
pub fn on_background_message(
    _messaging: &Messaging,
    _handler: MessageHandler,
) -> MessagingResult<Unsubscribe> {
    Err(available_in_service_worker(
        "on_background_message must be called in a Service Worker context (wasm target only)",
    ))
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
async fn get_token_native(
    messaging: &Messaging,
    vapid_key: Option<&str>,
) -> MessagingResult<String> {
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

#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
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
    let app_config =
        extract_app_config(messaging.app()).map_err(|err| internal_error(err.to_string()))?;
    let fcm_client = FcmClient::new()?;

    let mut sw_manager = ServiceWorkerManager::new();
    let registration = sw_manager.register_default().await?;
    let scope = registration.as_web_sys().scope();

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
    let subscription_payload = FcmSubscription {
        endpoint: &subscription_info.endpoint,
        auth: &subscription_info.auth,
        p256dh: &subscription_info.p256dh,
        application_pub_key: if subscription_info.vapid_key == DEFAULT_VAPID_KEY {
            None
        } else {
            Some(subscription_info.vapid_key.as_str())
        },
    };

    if let Some(record) = token_store::read_token(&store_key).await? {
        if let Some(existing) = &record.subscription {
            if existing == &subscription_info {
                let installation_needs_refresh = record.installation.auth_token_expired(now_ms);
                if !record.is_expired(now_ms, TOKEN_EXPIRATION_MS) && !installation_needs_refresh {
                    return Ok(record.token);
                }

                let installation_info =
                    fetch_installation_info(messaging, installation_needs_refresh).await?;
                let update_request = FcmUpdateRequest {
                    registration_token: &record.token,
                    registration: FcmRegistrationRequest {
                        project_id: &app_config.project_id,
                        api_key: &app_config.api_key,
                        installation_auth_token: &installation_info.auth_token,
                        subscription: subscription_payload.clone(),
                    },
                };

                let token = fcm_client.update_token(&update_request).await?;
                let record = TokenRecord {
                    token: token.clone(),
                    create_time_ms: now_ms,
                    subscription: Some(subscription_info),
                    installation: installation_info,
                };
                token_store::write_token(&store_key, &record).await?;
                return Ok(token);
            }
        }

        let installation_info = fetch_installation_info(messaging, true).await?;
        let _ = fcm_client
            .delete_token(
                &app_config.project_id,
                &app_config.api_key,
                &installation_info.auth_token,
                &record.token,
            )
            .await;
        let _ = token_store::remove_token(&store_key).await?;
    }

    let installation_info = fetch_installation_info(messaging, true).await?;
    let registration_request = FcmRegistrationRequest {
        project_id: &app_config.project_id,
        api_key: &app_config.api_key,
        installation_auth_token: &installation_info.auth_token,
        subscription: subscription_payload,
    };

    let token = fcm_client.register_token(&registration_request).await?;
    let record = TokenRecord {
        token: token.clone(),
        create_time_ms: now_ms,
        subscription: Some(subscription_info),
        installation: installation_info,
    };
    token_store::write_token(&store_key, &record).await?;
    Ok(token)
}

#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    not(feature = "experimental-indexed-db")
))]
async fn get_token_wasm(_: &Messaging, _: Option<&str>) -> MessagingResult<String> {
    Err(unsupported_browser(
        "Firebase Messaging token persistence requires the `experimental-indexed-db` feature on wasm targets.",
    ))
}

#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
async fn delete_token_impl(messaging: &Messaging) -> MessagingResult<bool> {
    use crate::messaging::subscription::PushSubscriptionManager;
    use crate::messaging::sw_manager::ServiceWorkerManager;

    let store_key = app_store_key(messaging);
    let record = match token_store::read_token(&store_key).await? {
        Some(record) => record,
        None => return Err(token_deletion_failed("No token stored for this app")),
    };

    let app_config =
        extract_app_config(messaging.app()).map_err(|err| internal_error(err.to_string()))?;
    let installation_info = fetch_installation_info(messaging, true).await?;
    let fcm_client = FcmClient::new()?;

    fcm_client
        .delete_token(
            &app_config.project_id,
            &app_config.api_key,
            &installation_info.auth_token,
            &record.token,
        )
        .await?;

    let removed = token_store::remove_token(&store_key).await?;

    let mut sw_manager = ServiceWorkerManager::new();
    let registration = sw_manager.register_default().await?;
    let sw_registration = registration.as_web_sys();
    let push_manager = sw_registration
        .push_manager()
        .map_err(|err| internal_error(format_js_error("pushManager", err)))?;
    let subscription_value = JsFuture::from(
        push_manager
            .get_subscription()
            .map_err(|err| internal_error(format_js_error("getSubscription", err)))?,
    )
    .await
    .map_err(|err| internal_error(format_js_error("getSubscription", err)))?;

    if !subscription_value.is_null() && !subscription_value.is_undefined() {
        let subscription: web_sys::PushSubscription = subscription_value
            .dyn_into()
            .map_err(|_| internal_error("PushManager.getSubscription returned unexpected value"))?;
        let promise = subscription
            .unsubscribe()
            .map_err(|err| internal_error(format_js_error("PushSubscription.unsubscribe", err)))?;
        let _ = JsFuture::from(promise)
            .await
            .map_err(|err| internal_error(format_js_error("PushSubscription.unsubscribe", err)))?;
    }

    let mut push_manager = PushSubscriptionManager::new();
    push_manager.clear_cache();

    Ok(removed)
}

#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    not(feature = "experimental-indexed-db")
))]
async fn delete_token_impl(_: &Messaging) -> MessagingResult<bool> {
    Err(token_deletion_failed(
        "Token deletion is unavailable without the `experimental-indexed-db` feature on wasm targets.",
    ))
}

#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
async fn fetch_installation_info(
    messaging: &Messaging,
    force_refresh: bool,
) -> MessagingResult<InstallationInfo> {
    let internal = get_installations_internal(Some(messaging.inner.app.clone()))
        .map_err(|err| internal_error(format!("Failed to initialise installations: {err}")))?;

    let InstallationEntryData {
        fid,
        refresh_token,
        mut auth_token,
    } = internal
        .get_installation_entry()
        .await
        .map_err(|err| internal_error(format!("Failed to load installation entry: {err}")))?;

    if force_refresh || auth_token.is_expired() {
        auth_token = internal.get_token(true).await.map_err(|err| {
            internal_error(format!("Failed to refresh installation token: {err}"))
        })?;
    }

    let expires_at_ms = system_time_to_millis(auth_token.expires_at)?;

    Ok(InstallationInfo {
        fid,
        refresh_token,
        auth_token: auth_token.token,
        auth_token_expiration_ms: expires_at_ms,
    })
}

#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
fn system_time_to_millis(time: SystemTime) -> MessagingResult<u64> {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .map_err(|_| internal_error("Installation token expiration precedes UNIX epoch"))
}

#[allow(dead_code)]
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

#[allow(dead_code)]
const TOKEN_EXPIRATION_MS: u64 = 7 * 24 * 60 * 60 * 1000;

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
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
    #[allow(unused_imports)]
    use std::task::Waker;
    use std::task::{RawWaker, RawWakerVTable};

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

    #[tokio::test(flavor = "current_thread")]
    async fn token_is_stable_until_deleted() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .expect("init app");
        let messaging = get_messaging(Some(app)).await.unwrap();
        let permission = messaging.request_permission().await.unwrap();
        assert_eq!(permission, PermissionState::Granted);
        let token1 = messaging.get_token(None).await.unwrap();
        let token2 = messaging.get_token(None).await.unwrap();
        assert_eq!(token1, token2);
        messaging.delete_token().await.unwrap();
        let token3 = messaging.get_token(None).await.unwrap();
        assert_ne!(token1, token3);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn get_token_with_empty_vapid_key_returns_error() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .expect("init app");
        let messaging = get_messaging(Some(app)).await.unwrap();
        let err = messaging.get_token(Some(" ")).await.unwrap_err();
        assert_eq!(err.code_str(), "messaging/invalid-argument");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn delete_token_without_existing_token_returns_error() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .expect("init app");
        let messaging = get_messaging(Some(app)).await.unwrap();
        let err = messaging.delete_token().await.unwrap_err();
        assert_eq!(err.code_str(), "messaging/token-deletion-failed");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn token_persists_across_messaging_instances() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .expect("init app");
        let messaging = get_messaging(Some(app.clone())).await.unwrap();
        let token1 = messaging.get_token(None).await.unwrap();

        // Re-fetch messaging for the same app and validate the stored token is reused.
        let messaging_again = get_messaging(Some(app)).await.unwrap();
        let token2 = messaging_again.get_token(None).await.unwrap();
        assert_eq!(token1, token2);

        messaging_again.delete_token().await.unwrap();
    }

    #[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
    #[tokio::test(flavor = "current_thread")]
    async fn on_message_returns_window_error_on_non_wasm() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .expect("init app");
        let messaging = get_messaging(Some(app)).await.unwrap();

        let handler: MessageHandler = Arc::new(|_| {});
        let err = match super::on_message(&messaging, handler) {
            Ok(unsub) => {
                unsub();
                panic!("expected on_message to fail on non-wasm targets");
            }
            Err(err) => err,
        };
        assert_eq!(err.code_str(), "messaging/available-in-window");
    }

    #[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
    #[tokio::test(flavor = "current_thread")]
    async fn on_background_message_returns_sw_error_on_non_wasm() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings()))
            .await
            .expect("init app");
        let messaging = get_messaging(Some(app)).await.unwrap();

        let handler: MessageHandler = Arc::new(|_| {});
        let err = match super::on_background_message(&messaging, handler) {
            Ok(unsub) => {
                unsub();
                panic!("expected on_background_message to fail on non-wasm targets");
            }
            Err(err) => err,
        };
        assert_eq!(err.code_str(), "messaging/available-in-sw");
    }

    #[allow(dead_code)]
    fn noop_raw_waker() -> RawWaker {
        RawWaker::new(std::ptr::null(), &NOOP_RAW_WAKER_VTABLE)
    }

    #[allow(dead_code)]
    unsafe fn noop_raw_waker_clone(_: *const ()) -> RawWaker {
        noop_raw_waker()
    }

    #[allow(dead_code)]
    unsafe fn noop_raw_waker_wake(_: *const ()) {}

    #[allow(dead_code)]
    unsafe fn noop_raw_waker_wake_by_ref(_: *const ()) {}

    #[allow(dead_code)]
    unsafe fn noop_raw_waker_drop(_: *const ()) {}

    #[allow(dead_code)]
    static NOOP_RAW_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
        noop_raw_waker_clone,
        noop_raw_waker_wake,
        noop_raw_waker_wake_by_ref,
        noop_raw_waker_drop,
    );
}
