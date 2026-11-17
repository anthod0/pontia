use axum::extract::FromRef;
use pontia_agent_control::workspaces::WorkspaceService;
use pontia_bot::identity::{BotIdentity, UnboundBotIdentity};
use pontia_bot::memory::{MemoryCommands, MemoryService};
use pontia_bot::{
    user::{UserCommands, UserService},
    workspaces::WorkspaceCommands,
};
use std::sync::Arc;

use pontia_application::AppState;

use super::dashboard::ResolvedDashboard;

#[derive(Clone)]
pub struct HttpState {
    app: AppState,
    dashboard: ResolvedDashboard,
    memory_commands: MemoryCommands,
    user_service: UserService,
    user_commands: UserCommands,
    workspace_commands: WorkspaceCommands,
}

impl HttpState {
    pub fn new(app: AppState, dashboard: ResolvedDashboard) -> Self {
        let memory_commands =
            MemoryCommands::new(MemoryService::new(app.db()), Arc::new(UnboundBotIdentity));
        let user_service = UserService::new(app.pontia_home());
        let user_commands = UserCommands::new(user_service.clone(), Arc::new(UnboundBotIdentity));
        let workspace_commands = WorkspaceCommands::new(
            WorkspaceService::new(app.db(), app.workspace_browser()),
            Arc::new(UnboundBotIdentity),
        );
        Self {
            user_service,
            user_commands,
            workspace_commands,
            app,
            dashboard,
            memory_commands,
        }
    }

    pub fn with_bot_identity(mut self, identity: Arc<dyn BotIdentity>) -> Self {
        self.memory_commands =
            MemoryCommands::new(MemoryService::new(self.app.db()), identity.clone());
        self.user_commands = UserCommands::new(self.user_service.clone(), identity.clone());
        self.workspace_commands = WorkspaceCommands::new(
            WorkspaceService::new(self.app.db(), self.app.workspace_browser()),
            identity,
        );
        self
    }

    pub fn memory_commands(&self) -> &MemoryCommands {
        &self.memory_commands
    }

    pub fn user_commands(&self) -> &UserCommands {
        &self.user_commands
    }

    pub fn workspace_commands(&self) -> &WorkspaceCommands {
        &self.workspace_commands
    }

    pub fn app(&self) -> &AppState {
        &self.app
    }

    pub fn dashboard(&self) -> &ResolvedDashboard {
        &self.dashboard
    }
}

impl From<AppState> for HttpState {
    fn from(app: AppState) -> Self {
        Self::new(app, ResolvedDashboard::local_default())
    }
}

impl FromRef<HttpState> for AppState {
    fn from_ref(state: &HttpState) -> Self {
        state.app.clone()
    }
}

impl FromRef<HttpState> for ResolvedDashboard {
    fn from_ref(state: &HttpState) -> Self {
        state.dashboard.clone()
    }
}
