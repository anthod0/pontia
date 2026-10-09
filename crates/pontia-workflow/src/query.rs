mod context;
mod graph;
mod overview;
mod patches;
mod timeline;

use pontia_storage_sqlite::repositories::{
    sessions::SqliteSessionRepository, turns::SqliteTurnRepository,
    workflows::SqliteWorkflowRepository,
};
use sqlx::SqlitePool;

pub use context::{
    WorkflowContextView, WorkflowDocumentView, WorkflowInputView, WorkflowNodeContextView,
};
pub use graph::{WorkflowGraphNodeView, WorkflowGraphRevisionView};
pub use overview::{
    WorkflowAgentStatus, WorkflowDetailView, WorkflowListItemView, WorkflowNodeView,
};
pub use patches::{WorkflowActivePatchView, WorkflowPatchHistoryView};
pub use timeline::{WorkflowTimelineEntryView, WorkflowTimelineView};

#[derive(Debug, Clone)]
pub struct WorkflowQueryService {
    workflows: SqliteWorkflowRepository,
    sessions: SqliteSessionRepository,
    turns: SqliteTurnRepository,
}

impl WorkflowQueryService {
    pub fn with_clients(mut self, clients: pontia_application::clients::ClientRegistry) -> Self {
        self.workflows = self.workflows.with_recovery_client_types(
            clients
                .registrations()
                .filter(|client| client.spec.adapter.lifecycle.confirmed_exit_recovery)
                .map(|client| client.spec.client_type),
        );
        self
    }
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            workflows: SqliteWorkflowRepository::new(pool.clone()),
            sessions: SqliteSessionRepository::new(pool.clone()),
            turns: SqliteTurnRepository::new(pool),
        }
    }
}
