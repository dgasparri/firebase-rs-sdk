use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock, Mutex};

use super::errors::{AppCheckError, AppCheckResult};
use super::types::{
    AppCheck, AppCheckProvider, AppCheckState, AppCheckToken, AppCheckTokenListener,
    AppCheckTokenResult, ListenerHandle, ListenerType, TokenListenerEntry,
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
        .map(Clone::clone)
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
    with_state(&app_name, |state| state.token.clone())
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

#[allow(dead_code)]
pub fn clear_state() {
    STATES.lock().unwrap().clear();
}
