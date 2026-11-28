//! Workflow orchestration over Agent Control.
pub mod models;
pub mod repository;
pub use repository::WorkflowState;

mod activation;
mod control;
mod coordinator;
mod error;
mod ports;
mod query;
mod recovery;
pub use recovery::WorkflowRecoveryService;
mod scheduler;
mod types;
mod validation;

pub use control::{WorkflowControlOutcome, WorkflowControlService};
pub use coordinator::WorkflowRuntime;
pub use error::{Error, Result};
pub use ports::{GracefulExitRequester, ProfileSessionConfig, SessionCreator};
pub use query::{
    WorkflowAgentStatus, WorkflowContextView, WorkflowDetailView, WorkflowInputView,
    WorkflowListItemView, WorkflowNodeContextView, WorkflowNodeView, WorkflowQueryService,
    WorkflowTimelineEntryView, WorkflowTimelineView,
};
pub use scheduler::WorkflowScheduler;
pub use types::{
    InitialHandoff, RunWorkflowOutcome, RunWorkflowRequest, StartWorkflowOutcome,
    SubmitWorkflowNodeRequest, WorkflowNodeDefinition,
};
