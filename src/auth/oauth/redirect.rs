use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::auth::error::AuthResult;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingRedirectEvent {
    pub provider_id: String,
    pub operation: RedirectOperation,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RedirectOperation {
    SignIn,
    Link,
}

pub trait RedirectPersistence: Send + Sync {
    fn set(&self, event: Option<PendingRedirectEvent>) -> AuthResult<()>;
    fn get(&self) -> AuthResult<Option<PendingRedirectEvent>>;
}

#[derive(Default, Debug)]
pub struct InMemoryRedirectPersistence {
    inner: Mutex<Option<PendingRedirectEvent>>,
}

impl RedirectPersistence for InMemoryRedirectPersistence {
    fn set(&self, event: Option<PendingRedirectEvent>) -> AuthResult<()> {
        *self.inner.lock().unwrap() = event;
        Ok(())
    }

    fn get(&self) -> AuthResult<Option<PendingRedirectEvent>> {
        Ok(self.inner.lock().unwrap().clone())
    }
}

impl InMemoryRedirectPersistence {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_persistence_round_trip() {
        let persistence = InMemoryRedirectPersistence::shared();
        let event = PendingRedirectEvent {
            provider_id: "google.com".into(),
            operation: RedirectOperation::Link,
        };

        persistence.set(Some(event.clone())).unwrap();
        assert_eq!(persistence.get().unwrap(), Some(event.clone()));

        persistence.set(None).unwrap();
        assert_eq!(persistence.get().unwrap(), None);
    }
}
