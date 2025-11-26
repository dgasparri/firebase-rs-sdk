use std::fs::{remove_file, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::auth::error::{AuthError, AuthResult};
use crate::auth::persistence::{AuthPersistence, PersistedAuthState, PersistenceListener, PersistenceSubscription};
use serde_json::{from_str as deserialize_state, to_string as serialize_state};

#[derive(Clone)]
pub struct FilePersistence {
    path: Arc<PathBuf>,
    listeners: Arc<Mutex<Vec<PersistenceListener>>>,
}

impl std::fmt::Debug for FilePersistence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilePersistence").field("path", &self.path).finish()
    }
}

impl FilePersistence {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: Arc::new(path.as_ref().to_path_buf()),
            listeners: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn notify_listeners(&self, state: Option<PersistedAuthState>) {
        let listeners = self.listeners.lock().unwrap().clone();
        for listener in listeners {
            listener(state.clone());
        }
    }
}

impl AuthPersistence for FilePersistence {
    fn set(&self, state: Option<PersistedAuthState>) -> AuthResult<()> {
        match &state {
            Some(state) => {
                let serialized = serialize_state(state).map_err(|err| {
                    AuthError::InvalidCredential(format!("Failed to serialize auth state for persistence: {err}"))
                })?;
                if let Some(parent) = self.path.parent() {
                    std::fs::create_dir_all(parent).map_err(|err| {
                        AuthError::InvalidCredential(format!("Failed to create persistence directory: {err}"))
                    })?;
                }
                let mut file = File::create(&*self.path).map_err(|err| {
                    AuthError::InvalidCredential(format!("Failed to create auth persistence file: {err}"))
                })?;
                file.write_all(serialized.as_bytes()).map_err(|err| {
                    AuthError::InvalidCredential(format!("Failed to write auth persistence file: {err}"))
                })?;
            }
            None => {
                if self.path.exists() {
                    remove_file(&*self.path).map_err(|err| {
                        AuthError::InvalidCredential(format!("Failed to remove auth persistence file: {err}"))
                    })?;
                }
            }
        }

        self.notify_listeners(state.clone());
        Ok(())
    }

    fn get(&self) -> AuthResult<Option<PersistedAuthState>> {
        if !self.path.exists() {
            return Ok(None);
        }

        let mut file = File::open(&*self.path)
            .map_err(|err| AuthError::InvalidCredential(format!("Failed to open auth persistence file: {err}")))?;
        let mut buffer = String::new();
        file.read_to_string(&mut buffer)
            .map_err(|err| AuthError::InvalidCredential(format!("Failed to read auth persistence file: {err}")))?;

        if buffer.is_empty() {
            return Ok(None);
        }

        let state = deserialize_state(&buffer)
            .map_err(|err| AuthError::InvalidCredential(format!("Failed to parse auth persistence payload: {err}")))?;
        Ok(Some(state))
    }

    fn subscribe(&self, listener: PersistenceListener) -> AuthResult<PersistenceSubscription> {
        let listener_arc = listener.clone();
        let mut listeners = self.listeners.lock().unwrap();
        listeners.push(listener_arc.clone());
        drop(listeners);

        let listeners = Arc::downgrade(&self.listeners);
        Ok(PersistenceSubscription::new(move || {
            if let Some(listeners) = listeners.upgrade() {
                let mut guard = listeners.lock().unwrap();
                guard.retain(|existing| !Arc::ptr_eq(existing, &listener_arc));
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("firebase-auth-test-{}-{}.json", name, std::process::id()));
        path
    }

    #[test]
    fn roundtrip_persistence() {
        let path = temp_path("roundtrip");
        let persistence = FilePersistence::new(&path);
        let state = PersistedAuthState {
            user_id: "user".into(),
            email: Some("user@example.com".into()),
            refresh_token: Some("refresh".into()),
            access_token: Some("access".into()),
            expires_at: Some(1234),
        };

        persistence.set(Some(state.clone())).unwrap();
        let loaded = persistence.get().unwrap();
        assert_eq!(loaded, Some(state.clone()));

        persistence.set(None).unwrap();
        assert!(persistence.get().unwrap().is_none());

        let _ = remove_file(path);
    }
}
