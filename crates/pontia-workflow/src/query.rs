mod context;
mod graph;
mod overview;
mod patches;
mod timeline;

use crate::repository::SqliteWorkflowRepository;
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

#[derive(Clone)]
pub struct WorkflowQueryService {
    workflows: SqliteWorkflowRepository,
    queries: pontia_agent_control::ControlQueryService,
}

impl WorkflowQueryService {
    pub fn with_clients(mut self, clients: pontia_agent_control::clients::ClientRegistry) -> Self {
        self.workflows = self.workflows.with_recovery_client_types(
            clients
                .registrations()
                .filter(|client| client.spec.adapter.lifecycle.confirmed_exit_recovery)
                .map(|client| client.spec.client_type),
        );
        self
    }
    pub fn new(pool: SqlitePool, queries: pontia_agent_control::ControlQueryService) -> Self {
        Self {
            workflows: SqliteWorkflowRepository::new(pool, queries.clone()),
            queries,
        }
    }
}
