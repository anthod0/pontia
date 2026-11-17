use axum::extract::FromRef;
use pontia_bot::memory::{MemoryCommands, MemoryIdentity, MemoryService, UnboundMemoryIdentity};
use std::sync::Arc;

use pontia_application::AppState;

use super::dashboard::ResolvedDashboard;

#[derive(Clone)]
pub struct HttpState {
    app: AppState,
    dashboard: ResolvedDashboard,
    memory_commands: MemoryCommands,
}

impl HttpState {
    pub fn new(app: AppState, dashboard: ResolvedDashboard) -> Self {
        let memory_commands = MemoryCommands::new(
            MemoryService::new(app.db()),
            Arc::new(UnboundMemoryIdentity),
        );
        Self {
            app,
            dashboard,
            memory_commands,
        }
    }

    pub fn with_memory_identity(mut self, identity: Arc<dyn MemoryIdentity>) -> Self {
        self.memory_commands = MemoryCommands::new(MemoryService::new(self.app.db()), identity);
        self
    }

    pub fn memory_commands(&self) -> &MemoryCommands {
        &self.memory_commands
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
