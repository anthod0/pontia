mod credentials;
mod launch;
mod snapshots;
pub use snapshots::{
    BusySessionSummary, OwnedSession, RuntimeSnapshot, SessionOwnership, SessionSnapshot,
    TurnSnapshot,
};
mod control;
pub(crate) use control::SessionControlState;
mod event_history;
mod events;
pub use event_history::{SessionEventSummary, TimePage};
mod sessions;
mod turns;
mod workspaces;
use sqlx::SqlitePool;
#[derive(Clone)]
pub struct ControlQueryService {
    clients: crate::clients::ClientRegistry,
    pool: SqlitePool,
    workspace_browser: pontia_config::WorkspaceBrowserConfig,
}
impl ControlQueryService {
    pub fn with_workspace_browser(mut self, config: pontia_config::WorkspaceBrowserConfig) -> Self {
        self.workspace_browser = config;
        self
    }
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            workspace_browser: Default::default(),
            pool,
            clients: Default::default(),
        }
    }
    pub fn with_clients(mut self, clients: crate::clients::ClientRegistry) -> Self {
        self.clients = clients;
        self
    }
}

mod client_sessions;
pub use client_sessions::{ClientSessionFields, SessionLaunchFields, SessionRuntimeRecord};
mod facts;
pub use facts::{CompletedTurnWithBoundaries, ReportEvent};
