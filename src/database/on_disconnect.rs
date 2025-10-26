use serde_json::{Map as JsonMap, Value};

use crate::database::api::{normalize_path, pack_with_priority, validate_priority_value};
use crate::database::error::{invalid_argument, DatabaseResult};
use crate::database::DatabaseReference;

/// Async handle for scheduling writes that run when the client disconnects.
///
/// Mirrors the surface of the JS SDK (`packages/database/src/api/OnDisconnect.ts`).
/// Operations resolve server value placeholders before being sent to the realtime
/// backend and require an active WebSocket transport for full server-side
/// semantics. When the runtime falls back to the HTTP long-poll transport the
/// commands are queued locally and flushed when `Database::go_offline()` runs,
/// which preserves graceful shutdowns but cannot detect abrupt connection loss.
#[derive(Clone, Debug)]
pub struct OnDisconnect {
    reference: DatabaseReference,
}

impl OnDisconnect {
    pub(crate) fn new(reference: DatabaseReference) -> Self {
        Self { reference }
    }

    /// Schedules a write for when the client disconnects.
    ///
    /// Mirrors `OnDisconnect.set()` from the JS SDK. The payload is normalised
    /// using the same server timestamp/increment resolution as immediate writes
    /// so placeholders resolve against the current backend value.
    pub async fn set<V>(&self, value: V) -> DatabaseResult<()>
    where
        V: Into<Value>,
    {
        let resolved = self
            .reference
            .resolve_for_current_path(value.into())
            .await?;
        self.reference
            .database()
            .repo()
            .on_disconnect_put(self.reference.path_segments(), resolved)
            .await
    }

    /// Schedules a write together with its priority for disconnect.
    pub async fn set_with_priority<V, P>(&self, value: V, priority: P) -> DatabaseResult<()>
    where
        V: Into<Value>,
        P: Into<Value>,
    {
        let priority = priority.into();
        validate_priority_value(&priority)?;
        if matches!(self.reference.key(), Some(".length" | ".keys")) {
            return Err(invalid_argument(
                "set_with_priority failed: read-only child key",
            ));
        }

        let resolved = self
            .reference
            .resolve_for_current_path(value.into())
            .await?;
        let payload = pack_with_priority(resolved, priority);
        self.reference
            .database()
            .repo()
            .on_disconnect_put(self.reference.path_segments(), payload)
            .await
    }

    /// Schedules an update when the client disconnects.
    pub async fn update(&self, updates: JsonMap<String, Value>) -> DatabaseResult<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let base_path = self.reference.path_segments();
        let mut payload = JsonMap::with_capacity(updates.len());

        for (key, value) in updates {
            if key.trim().is_empty() {
                return Err(invalid_argument("OnDisconnect.update keys cannot be empty"));
            }
            let relative_segments = normalize_path(&key)?;
            if relative_segments.is_empty() {
                return Err(invalid_argument(
                    "OnDisconnect.update path cannot reference the current location",
                ));
            }

            let mut absolute = base_path.clone();
            absolute.extend(relative_segments.clone());
            let resolved = self
                .reference
                .resolve_for_absolute_path(&absolute, value)
                .await?;
            let canonical = relative_segments.join("/");
            payload.insert(canonical, resolved);
        }

        self.reference
            .database()
            .repo()
            .on_disconnect_merge(base_path, Value::Object(payload))
            .await
    }

    /// Ensures the value at this location is deleted when the client disconnects.
    pub async fn remove(&self) -> DatabaseResult<()> {
        self.set(Value::Null).await
    }

    /// Cancels all pending on-disconnect operations.
    pub async fn cancel(&self) -> DatabaseResult<()> {
        self.reference
            .database()
            .repo()
            .on_disconnect_cancel(self.reference.path_segments())
            .await
    }
}
