use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::app::FirebaseApp;
use crate::database::error::DatabaseResult;

#[async_trait(?Send)]
pub trait RealtimeTransport: Send + Sync {
    async fn connect(&self) -> DatabaseResult<()>;
    async fn disconnect(&self) -> DatabaseResult<()>;
}

#[derive(Debug, Default)]
enum RepoState {
    #[default]
    Offline,
    Online,
}

#[derive(Clone)]
pub struct Repo {
    transport: Arc<dyn RealtimeTransport>,
    state: Arc<Mutex<RepoState>>,
}

impl Repo {
    pub fn new_for_app(_app: &FirebaseApp) -> Arc<Self> {
        Arc::new(Self {
            transport: Arc::new(NoopTransport),
            state: Arc::new(Mutex::new(RepoState::Offline)),
        })
    }

    pub async fn go_online(&self) -> DatabaseResult<()> {
        self.transport.connect().await?;
        *self.state.lock().unwrap() = RepoState::Online;
        Ok(())
    }

    pub async fn go_offline(&self) -> DatabaseResult<()> {
        self.transport.disconnect().await?;
        *self.state.lock().unwrap() = RepoState::Offline;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct NoopTransport;

#[async_trait(?Send)]
impl RealtimeTransport for NoopTransport {
    async fn connect(&self) -> DatabaseResult<()> {
        Ok(())
    }

    async fn disconnect(&self) -> DatabaseResult<()> {
        Ok(())
    }
}
