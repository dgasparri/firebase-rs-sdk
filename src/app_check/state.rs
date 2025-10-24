use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock, Mutex};

use super::errors::{AppCheckError, AppCheckResult};
use super::types::{
    AppCheck, AppCheckProvider, AppCheckState, AppCheckToken, AppCheckTokenListener,
    AppCheckTokenResult, ListenerHandle, ListenerType, TokenListenerEntry,
};

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use crate::app_check::persistence::{
    load_token, save_token, subscribe, BroadcastSubscription, PersistedToken,
};

static STATES: LazyLock<Mutex<HashMap<Arc<str>, AppCheckState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn with_state_mut<F, R>(app_name: &Arc<str>, mut f: F) -> R
where
    F: FnMut(&mut AppCheckState) -> R,
{
    let mut guard = STATES.lock().unwrap();
    let state = guard
        .entry(app_name.clone())
        .or_insert_with(AppCheckState::new);
    f(state)
}

fn with_state<F, R>(app_name: &Arc<str>, mut f: F) -> R
where
    F: FnMut(&AppCheckState) -> R,
{
    let guard = STATES.lock().unwrap();
    let state = guard
        .get(app_name)
        .cloned()
        .unwrap_or_else(AppCheckState::new);
    f(&state)
}

pub fn ensure_activation(
    app: &AppCheck,
    provider: Arc<dyn AppCheckProvider>,
    is_token_auto_refresh_enabled: bool,
) -> AppCheckResult<()> {
    let app_name = app.app_name();
    with_state_mut(&app_name, |state| {
        if state.activated {
            if state.is_token_auto_refresh_enabled == is_token_auto_refresh_enabled
                && state
                    .provider
                    .as_ref()
                    .map(|existing| Arc::ptr_eq(existing, &provider))
                    .unwrap_or(false)
            {
                return Ok(());
            }
            return Err(AppCheckError::AlreadyInitialized {
                app_name: app.app().name().to_owned(),
            });
        }

        state.activated = true;
        state.provider = Some(provider.clone());
        state.is_token_auto_refresh_enabled = is_token_auto_refresh_enabled;
        #[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
        {
            if state.broadcast_handle.is_none() {
                let app_name_clone = app_name.clone();
                let callback: Arc<dyn Fn(Option<PersistedToken>) + Send + Sync> =
                    Arc::new(move |persisted| {
                        apply_persisted_token(app_name_clone.clone(), persisted);
                    });
                state.broadcast_handle = subscribe(app_name.clone(), callback);
            }
        }
        Ok(())
    })
}

pub fn is_activated(app: &AppCheck) -> bool {
    let app_name = app.app_name();
    with_state(&app_name, |state| state.activated)
}

pub fn provider(app: &AppCheck) -> Option<Arc<dyn AppCheckProvider>> {
    let app_name = app.app_name();
    with_state(&app_name, |state| state.provider.clone())
}

pub fn current_token(app: &AppCheck) -> Option<AppCheckToken> {
    let app_name = app.app_name();
    let token = with_state(&app_name, |state| state.token.clone());

    #[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
    if token.is_none() {
        let app_name_clone = app_name.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(Some(persisted)) = load_token(app_name_clone.as_ref()).await {
                apply_persisted_token(app_name_clone, Some(persisted));
            }
        });
    }

    token
}

pub fn store_token(app: &AppCheck, token: AppCheckToken) {
    let app_name = app.app_name();
    let result = AppCheckTokenResult::from_token(token.clone());
    let listeners = with_state_mut(&app_name, |state| {
        state.token = Some(token.clone());
        state
            .observers
            .iter()
            .map(|entry| entry.listener.clone())
            .collect::<Vec<_>>()
    });

    crate::app_check::api::on_token_stored(app, &token);

    #[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
    persist_token_async(token.clone(), app_name.clone());

    for listener in listeners {
        listener(&result);
    }
}

pub fn set_auto_refresh(app: &AppCheck, enabled: bool) {
    let app_name = app.app_name();
    with_state_mut(&app_name, |state| {
        state.is_token_auto_refresh_enabled = enabled;
    });
}

pub(crate) fn replace_refresh_cancel(
    app: &AppCheck,
    cancel: Option<Arc<AtomicBool>>,
) -> Option<Arc<AtomicBool>> {
    let app_name = app.app_name();
    with_state_mut(&app_name, |state| {
        let previous = state.refresh_cancel.clone();
        state.refresh_cancel = cancel.clone();
        previous
    })
}

#[allow(dead_code)]
pub fn auto_refresh_enabled(app: &AppCheck) -> bool {
    let app_name = app.app_name();
    with_state(&app_name, |state| state.is_token_auto_refresh_enabled)
}

pub fn add_listener(
    app: &AppCheck,
    listener: AppCheckTokenListener,
    listener_type: ListenerType,
) -> ListenerHandle {
    let app_name = app.app_name();
    let entry = TokenListenerEntry::new(listener, listener_type);
    let id = entry.id;
    with_state_mut(&app_name, |state| {
        state.observers.push(entry.clone());
    });

    let remover_name = app_name.clone();
    let unsubscribed = Arc::new(AtomicBool::new(false));
    let remover = Arc::new(move |listener_id: u64| {
        remove_listener_by_id(&remover_name, listener_id);
    });

    ListenerHandle {
        app_name,
        id,
        remover,
        unsubscribed,
    }
}

#[allow(dead_code)]
pub fn remove_listener(handle: &ListenerHandle) {
    remove_listener_by_id(&handle.app_name, handle.id);
}

fn remove_listener_by_id(app_name: &Arc<str>, listener_id: u64) {
    with_state_mut(app_name, |state| {
        state.observers.retain(|entry| entry.id != listener_id);
    });
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
fn persist_token_async(token: AppCheckToken, app_name: Arc<str>) {
    use std::time::UNIX_EPOCH;

    wasm_bindgen_futures::spawn_local(async move {
        let persisted = PersistedToken {
            token: token.token,
            expire_time_ms: token
                .expire_time
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or(0),
        };
        let _ = save_token(app_name.as_ref(), &persisted).await;
    });
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
fn apply_persisted_token(app_name: Arc<str>, persisted: Option<PersistedToken>) {
    use std::time::{Duration, UNIX_EPOCH};

    let maybe_token = persisted.map(|persisted| {
        let expiration = UNIX_EPOCH + Duration::from_millis(persisted.expire_time_ms);
        AppCheckToken::new(persisted.token, expiration)
    });

    let result = maybe_token
        .as_ref()
        .map(|token| AppCheckTokenResult::from_token(token.clone()))
        .unwrap_or_else(|| AppCheckTokenResult {
            token: String::new(),
            error: None,
            internal_error: None,
        });

    let listeners = with_state_mut(&app_name, |state| {
        state.token = maybe_token.clone();
        state
            .observers
            .iter()
            .map(|entry| entry.listener.clone())
            .collect::<Vec<_>>()
    });

    for listener in listeners {
        listener(&result);
    }
}

#[allow(dead_code)]
pub fn clear_state() {
    STATES.lock().unwrap().clear();
}
