use crate::database::error::{internal_error, DatabaseResult};

/// Minimal placeholder for the Realtime Database `PersistentConnection` port.
///
/// The JavaScript SDK uses `PersistentConnection` to manage WebSocket or long-poll
/// connections (`packages/database/src/core/PersistentConnection.ts`). The Rust port
/// currently exposes the type so downstream work can hook into it, but the
/// implementation is still pending.
#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct PersistentConnection {
    state: ConnectionState,
}

#[allow(dead_code)]
#[derive(Debug, Default)]
enum ConnectionState {
    #[default]
    Idle,
}

#[allow(dead_code)]
impl PersistentConnection {
    pub fn new() -> Self {
        Self {
            state: ConnectionState::Idle,
        }
    }

    pub fn connect(&mut self) -> DatabaseResult<()> {
        Err(internal_error(
            "Realtime transport not implemented – WebSocket integration pending",
        ))
    }
}

/// Skeleton for the `Repo` abstraction that orchestrates connections and writes.
///
/// Mirrors the surface of `packages/database/src/core/Repo.ts` but only provides the
/// structural placeholder so the rest of the port can compile and evolve.
#[allow(dead_code)]
#[derive(Debug)]
pub struct Repo {
    connection: PersistentConnection,
}

#[allow(dead_code)]
impl Repo {
    pub fn new() -> Self {
        Self {
            connection: PersistentConnection::new(),
        }
    }

    pub fn start(&mut self) -> DatabaseResult<()> {
        self.connection.connect()
    }
}
