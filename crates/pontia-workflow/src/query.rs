mod context;
mod graph;
mod overview;
mod timeline;

use crate::repository::WorkflowState;

pub use context::{WorkflowContextView, WorkflowInputView, WorkflowNodeContextView};
pub use overview::{
    WorkflowAgentStatus, WorkflowDetailView, WorkflowListItemView, WorkflowNodeView,
};
pub use timeline::{WorkflowTimelineEntryView, WorkflowTimelineView};

#[derive(Clone)]
pub struct WorkflowQueryService {
    workflows: WorkflowState,
    queries: pontia_agent_control::ControlQueryService,
}

impl WorkflowQueryService {
    pub fn new(workflows: WorkflowState) -> Self {
        Self {
            queries: workflows.queries.clone(),
            workflows,
        }
    }
}
