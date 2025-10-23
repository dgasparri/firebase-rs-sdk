use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::auth::error::AuthResult;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PersistedAuthState {
    pub user_id: String,
    pub email: Option<String>,
    pub refresh_token: Option<String>,
    pub access_token: Option<String>,
    /// Expiration timestamp in seconds since the Unix epoch.
    pub expires_at: Option<i64>,
}

pub type PersistenceListener = Arc<dyn Fn(Option<PersistedAuthState>) + Send + Sync>;

#[derive(Default)]
struct InMemoryState {
    value: Option<PersistedAuthState>,
    listeners: Vec<(usize, PersistenceListener)>,
}

pub struct PersistenceSubscription {
    cleanup: Option<Box<dyn FnOnce() + Send + 'static>>,
}

impl PersistenceSubscription {
    /// Creates a subscription with a cleanup callback that runs on drop.
    pub fn new<F>(cleanup: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        Self {
            cleanup: Some(Box::new(cleanup)),
        }
    }

    /// Returns a subscription that performs no cleanup work.
    pub fn noop() -> Self {
        Self { cleanup: None }
    }
}

impl Default for PersistenceSubscription {
    fn default() -> Self {
        Self::noop()
    }
}

impl Drop for PersistenceSubscription {
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            cleanup();
        }
    }
}

/// Storage backend for serialized authentication state.
///
/// Library consumers can implement this trait to plug in platform-specific
/// persistence (filesystem, databases, JS shims, etc.). Callbacks registered via
/// `subscribe` should emit whenever the underlying state changes outside the
/// current process so multi-instance listeners stay in sync.
pub trait AuthPersistence: Send + Sync {
    fn set(&self, state: Option<PersistedAuthState>) -> AuthResult<()>;
    fn get(&self) -> AuthResult<Option<PersistedAuthState>>;

    fn subscribe(&self, _listener: PersistenceListener) -> AuthResult<PersistenceSubscription> {
        Ok(PersistenceSubscription::noop())
    }
}

pub struct InMemoryPersistence {
    state: Arc<Mutex<InMemoryState>>,
    next_id: AtomicUsize,
}

impl Default for InMemoryPersistence {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(InMemoryState::default())),
            next_id: AtomicUsize::new(1),
        }
    }
}

impl AuthPersistence for InMemoryPersistence {
    fn set(&self, state: Option<PersistedAuthState>) -> AuthResult<()> {
        let listeners = {
            let mut guard = self.state.lock().unwrap();
            guard.value = state.clone();
            guard
                .listeners
                .iter()
                .map(|(_, listener)| listener.clone())
                .collect::<Vec<_>>()
        };

        for listener in listeners {
            listener(state.clone());
        }

        Ok(())
    }

    fn get(&self) -> AuthResult<Option<PersistedAuthState>> {
        Ok(self.state.lock().unwrap().value.clone())
    }

    fn subscribe(&self, listener: PersistenceListener) -> AuthResult<PersistenceSubscription> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        {
            let mut guard = self.state.lock().unwrap();
            guard.listeners.push((id, listener));
        }

        let state = Arc::downgrade(&self.state);
        Ok(PersistenceSubscription::new(move || {
            if let Some(state) = state.upgrade() {
                if let Ok(mut guard) = state.lock() {
                    guard
                        .listeners
                        .retain(|(listener_id, _)| *listener_id != id);
                }
            }
        }))
    }
}

type DynSetFn = dyn Fn(Option<PersistedAuthState>) -> AuthResult<()> + Send + Sync;
type DynGetFn = dyn Fn() -> AuthResult<Option<PersistedAuthState>> + Send + Sync;
type DynSubscribeFn =
    dyn Fn(PersistenceListener) -> AuthResult<PersistenceSubscription> + Send + Sync;

pub struct ClosurePersistence {
    set_fn: Arc<DynSetFn>,
    get_fn: Arc<DynGetFn>,
    subscribe_fn: Arc<DynSubscribeFn>,
}

impl ClosurePersistence {
    /// Creates a persistence backend from set/get closures.
    pub fn new<Set, Get>(set: Set, get: Get) -> Self
    where
        Set: Fn(Option<PersistedAuthState>) -> AuthResult<()> + Send + Sync + 'static,
        Get: Fn() -> AuthResult<Option<PersistedAuthState>> + Send + Sync + 'static,
    {
        Self::with_subscribe(set, get, |_| Ok(PersistenceSubscription::noop()))
    }

    /// Creates a persistence backend with custom set/get/subscribe behavior.
    pub fn with_subscribe<Set, Get, Subscribe>(set: Set, get: Get, subscribe: Subscribe) -> Self
    where
        Set: Fn(Option<PersistedAuthState>) -> AuthResult<()> + Send + Sync + 'static,
        Get: Fn() -> AuthResult<Option<PersistedAuthState>> + Send + Sync + 'static,
        Subscribe:
            Fn(PersistenceListener) -> AuthResult<PersistenceSubscription> + Send + Sync + 'static,
    {
        Self {
            set_fn: Arc::new(set),
            get_fn: Arc::new(get),
            subscribe_fn: Arc::new(subscribe),
        }
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
mod indexed_db;

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
pub use indexed_db::IndexedDbPersistence;

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
mod file;

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
pub use file::FilePersistence;

impl AuthPersistence for ClosurePersistence {
    fn set(&self, state: Option<PersistedAuthState>) -> AuthResult<()> {
        (self.set_fn)(state)
    }

    fn get(&self) -> AuthResult<Option<PersistedAuthState>> {
        (self.get_fn)()
    }

    fn subscribe(&self, listener: PersistenceListener) -> AuthResult<PersistenceSubscription> {
        (self.subscribe_fn)(listener)
    }
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
pub mod web;

#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
pub use web::{WebStorageDriver, WebStoragePersistence};
