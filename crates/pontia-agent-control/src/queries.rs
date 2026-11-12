mod domain_events;
mod snapshots;
pub use domain_events::{DomainEventPage, DomainEventReader, DomainEventRecord};
pub use snapshots::{RuntimeSnapshot, SessionOwnership, SessionSnapshot, TurnSnapshot};
mod control;
pub(crate) use control::SessionControlState;
mod events;
mod sessions;
mod turns;
mod workspaces;
use sqlx::SqlitePool;
#[derive(Clone)]
pub struct ControlQueryService {
    clients: crate::clients::ClientRegistry,
    pool: SqlitePool,
    workspace_browser: pontia_config::WorkspaceBrowserConfig,
    control: Option<crate::ClientControlService>,
}
impl ControlQueryService {
    pub fn domain_events(&self) -> DomainEventReader {
        DomainEventReader::new(self.pool.clone())
    }

    pub fn with_workspace_browser(mut self, config: pontia_config::WorkspaceBrowserConfig) -> Self {
        self.workspace_browser = config;
        self
    }
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            workspace_browser: Default::default(),
            pool,
            clients: Default::default(),
            control: None,
        }
    }
    pub fn with_clients(mut self, clients: crate::clients::ClientRegistry) -> Self {
        self.clients = clients;
        self
    }
    pub fn with_control(mut self, control: crate::ClientControlService) -> Self {
        self.control = Some(control);
        self
    }
}
