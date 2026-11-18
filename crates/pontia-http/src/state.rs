use axum::extract::FromRef;
use pontia_agent_control::TurnTimelineService;
use pontia_agent_control::workspaces::WorkspaceService;
use pontia_bot::conversation::ConversationService;
use pontia_bot::identity::BotIdentity;
use pontia_bot::memory::{MemoryCommands, MemoryService};
use pontia_bot::session_messages::SessionMessages;
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
    session_messages: SessionMessages,
    bots: ConversationService,
    bot_identity: Arc<dyn BotIdentity>,
}

impl HttpState {
    pub fn new(app: AppState, dashboard: ResolvedDashboard) -> Self {
        let bots = ConversationService::new(app.control().clone());
        let identity: Arc<dyn BotIdentity> = Arc::new(bots.clone());
        let memory_commands = MemoryCommands::new(MemoryService::new(app.db()), identity.clone());
        let user_service = UserService::new(app.pontia_home());
        let user_commands = UserCommands::new(user_service.clone(), identity.clone());
        let workspace_commands = WorkspaceCommands::new(
            WorkspaceService::new(app.db(), app.workspace_browser()),
            identity.clone(),
        );
        let session_messages = SessionMessages::new(
            TurnTimelineService::new(app.control().event_ingest_service()),
            identity.clone(),
        );
        Self {
            session_messages,
            user_service,
            user_commands,
            workspace_commands,
            app,
            dashboard,
            memory_commands,
            bots,
            bot_identity: identity,
        }
    }

    pub fn with_bot_identity(mut self, identity: Arc<dyn BotIdentity>) -> Self {
        self.memory_commands =
            MemoryCommands::new(MemoryService::new(self.app.db()), identity.clone());
        self.user_commands = UserCommands::new(self.user_service.clone(), identity.clone());
        self.session_messages = SessionMessages::new(
            TurnTimelineService::new(self.app.control().event_ingest_service()),
            identity.clone(),
        );
        self.workspace_commands = WorkspaceCommands::new(
            WorkspaceService::new(self.app.db(), self.app.workspace_browser()),
            identity.clone(),
        );
        self.bot_identity = identity;
        self
    }

    pub fn memory_commands(&self) -> &MemoryCommands {
        &self.memory_commands
    }

    pub fn bots(&self) -> &ConversationService {
        &self.bots
    }

    pub(crate) fn bot_identity(&self) -> &dyn BotIdentity {
        self.bot_identity.as_ref()
    }

    pub fn user_commands(&self) -> &UserCommands {
        &self.user_commands
    }

    pub fn session_messages(&self) -> &SessionMessages {
        &self.session_messages
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
