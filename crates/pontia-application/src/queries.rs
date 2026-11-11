mod control;
pub(crate) use control::SessionControlState;
use sqlx::SqlitePool;

mod events;
mod git_status;
mod sessions;
pub use sessions::{SessionOverviewRequest, SessionOverviewView};
mod turns;
mod workspaces;

#[derive(Clone)]
pub struct ExternalQueryService {
    clients: crate::clients::ClientRegistry,
    pool: SqlitePool,
    control: Option<crate::ClientControlService>,
    workspace_browser: pontia_config::WorkspaceBrowserConfig,
}

impl ExternalQueryService {
    pub fn with_workspace_browser(mut self, config: pontia_config::WorkspaceBrowserConfig) -> Self {
        self.workspace_browser = config;
        self
    }

    pub fn with_control(mut self, control: crate::ClientControlService) -> Self {
        self.control = Some(control);
        self
    }

    pub fn with_clients(mut self, clients: crate::clients::ClientRegistry) -> Self {
        self.clients = clients;
        self
    }

    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            control: None,
            clients: Default::default(),
            workspace_browser: Default::default(),
        }
    }
}
