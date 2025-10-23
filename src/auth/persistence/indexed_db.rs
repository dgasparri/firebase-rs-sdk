use std::sync::Arc;

use crate::auth::error::{AuthError, AuthResult};
use crate::auth::persistence::{
    AuthPersistence, PersistedAuthState, PersistenceListener, PersistenceSubscription,
};
use crate::platform::browser::indexed_db::{
    delete_key, get_string, open_database_with_store, put_string, IndexedDbError,
};
use crate::util::runtime::block_on;
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
}

impl IndexedDbPersistence {
    pub fn new() -> Self {
        Self::with_names(DB_NAME, STORE_NAME)
    }

    pub fn with_names(db: impl Into<String>, store: impl Into<String>) -> Self {
        Self {
            db_name: Arc::new(db.into()),
            store_name: Arc::new(store.into()),
        }
    }

    async fn open_database(&self) -> Result<web_sys::IdbDatabase, AuthError> {
        open_database_with_store(&self.db_name, DB_VERSION, &self.store_name)
            .await
            .map_err(map_error)
    }
}

impl AuthPersistence for IndexedDbPersistence {
    fn set(&self, state: Option<PersistedAuthState>) -> AuthResult<()> {
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
        let db_name = self.db_name.clone();
        let store_name = self.store_name.clone();
        let future = async move {
            let db = open_database_with_store(&db_name, DB_VERSION, &store_name)
                .await
                .map_err(map_error)?;
            let value = get_string(&db, &store_name, AUTH_STATE_KEY)
                .await
                .map_err(map_error)?;
            let state = match value {
                Some(payload) if !payload.is_empty() => {
                    deserialize_state(&payload).map(Some).map_err(|err| {
                        AuthError::InvalidCredential(format!(
                            "Failed to parse persisted auth payload: {err}"
                        ))
                    })?
                }
                _ => None,
            };
            Ok(state)
        };
        block_on(future)
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
