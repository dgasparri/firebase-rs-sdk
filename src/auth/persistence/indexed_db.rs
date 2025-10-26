use std::sync::{Arc, Mutex};

use crate::auth::error::{AuthError, AuthResult};
use crate::auth::persistence::{
    AuthPersistence, PersistedAuthState, PersistenceListener, PersistenceSubscription,
};
use crate::platform::browser::indexed_db::{
    delete_key, get_string, open_database_with_store, put_string, IndexedDbError,
};
use serde_json::{from_str as deserialize_state, to_string as serialize_state};
use wasm_bindgen_futures::spawn_local;

const DB_NAME: &str = "firebase-auth";
const STORE_NAME: &str = "auth-store";
const DB_VERSION: u32 = 1;
const AUTH_STATE_KEY: &str = "firebase-auth-state";

#[derive(Clone, Debug)]
pub struct IndexedDbPersistence {
    db_name: Arc<String>,
    store_name: Arc<String>,
    cache: Arc<Mutex<Option<PersistedAuthState>>>,
}

impl IndexedDbPersistence {
    pub fn new() -> Self {
        Self::with_names(DB_NAME, STORE_NAME)
    }

    pub fn with_names(db: impl Into<String>, store: impl Into<String>) -> Self {
        let db_name = Arc::new(db.into());
        let store_name = Arc::new(store.into());
        let cache = Arc::new(Mutex::new(load_from_local_storage(&db_name)));
        Self {
            db_name,
            store_name,
            cache,
        }
    }

    #[allow(dead_code)]
    async fn open_database(&self) -> Result<web_sys::IdbDatabase, AuthError> {
        open_database_with_store(&self.db_name, DB_VERSION, &self.store_name)
            .await
            .map_err(map_error)
    }
}

impl AuthPersistence for IndexedDbPersistence {
    fn set(&self, state: Option<PersistedAuthState>) -> AuthResult<()> {
        {
            let mut cache = self.cache.lock().unwrap();
            *cache = state.clone();
        }
        write_to_local_storage(&self.db_name, &state);

        let db_name = self.db_name.clone();
        let store_name = self.store_name.clone();
        spawn_local(async move {
            let db = match open_database_with_store(&db_name, DB_VERSION, &store_name).await {
                Ok(db) => db,
                Err(_) => return,
            };

            let result = if let Some(state) = state {
                match serialize_state(&state) {
                    Ok(serialized) => {
                        put_string(&db, &store_name, AUTH_STATE_KEY, &serialized).await
                    }
                    Err(_) => return,
                }
            } else {
                delete_key(&db, &store_name, AUTH_STATE_KEY).await
            };

            if result.is_err() {
                // best-effort; swallow errors.
            }
        });
        Ok(())
    }

    fn get(&self) -> AuthResult<Option<PersistedAuthState>> {
        // Refresh cache from local storage on each read in case another tab updated it.
        {
            let mut cache = self.cache.lock().unwrap();
            *cache = load_from_local_storage(&self.db_name);
            Ok(cache.clone())
        }
    }

    fn subscribe(&self, _listener: PersistenceListener) -> AuthResult<PersistenceSubscription> {
        // IndexedDB does not expose a simple cross-tab notification mechanism without
        // additional BroadcastChannel wiring. Defer to higher-level coordination for now.
        Ok(PersistenceSubscription::noop())
    }
}

fn map_error(error: IndexedDbError) -> AuthError {
    AuthError::InvalidCredential(format!("IndexedDB auth persistence error: {error}"))
}

fn storage_key(db_name: &str) -> String {
    format!("{db_name}::{AUTH_STATE_KEY}")
}

fn write_to_local_storage(db_name: &str, state: &Option<PersistedAuthState>) {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            let key = storage_key(db_name);
            let _ = match state {
                Some(state) => serialize_state(state)
                    .map(|json| storage.set_item(&key, &json))
                    .unwrap_or_else(|_| Ok(())),
                None => storage.remove_item(&key),
            };
        }
    }
}

fn load_from_local_storage(db_name: &str) -> Option<PersistedAuthState> {
    let window = web_sys::window()?;
    let storage = window.local_storage().ok().flatten()?;
    let key = storage_key(db_name);
    let value = storage.get_item(&key).ok().flatten()?;
    if value.is_empty() {
        return None;
    }
    let parsed = deserialize_state(&value).ok()?;
    Some(parsed)
}
