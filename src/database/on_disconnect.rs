use serde_json::Value;

use crate::database::error::{internal_error, DatabaseResult};
use crate::database::DatabaseReference;

/// Placeholder implementation of the Realtime Database `OnDisconnect` API.
///
/// Mirrors the surface of the JS SDK (`packages/database/src/api/OnDisconnect.ts`),
/// but currently returns an error until realtime transports are implemented.
#[derive(Clone, Debug)]
pub struct OnDisconnect {
    reference: DatabaseReference,
}

impl OnDisconnect {
    pub(crate) fn new(reference: DatabaseReference) -> Self {
        Self { reference }
    }

    /// Schedules a write for when the client disconnects. Not yet implemented.
    pub fn set(&self, _value: Value) -> DatabaseResult<()> {
        Err(internal_error(format!(
            "OnDisconnect operations require realtime transport (path: {})",
            self.reference.path()
        )))
    }

    /// Schedules an update when the client disconnects. Not yet implemented.
    pub fn update(&self, _updates: Value) -> DatabaseResult<()> {
        Err(internal_error(format!(
            "OnDisconnect operations require realtime transport (path: {})",
            self.reference.path()
        )))
    }

    /// Schedules a remove when the client disconnects. Not yet implemented.
    pub fn remove(&self) -> DatabaseResult<()> {
        Err(internal_error(format!(
            "OnDisconnect operations require realtime transport (path: {})",
            self.reference.path()
        )))
    }

    /// Cancels all pending on-disconnect operations. Not yet implemented.
    pub fn cancel(&self) -> DatabaseResult<()> {
        Err(internal_error(format!(
            "OnDisconnect operations require realtime transport (path: {})",
            self.reference.path()
        )))
    }
}
