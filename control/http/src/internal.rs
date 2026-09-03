//! Internal HTTP API handlers and protocol types.
//!
//! The facade keeps router-facing paths stable while private responsibility
//! modules own each endpoint family and its policies.

mod agent_binding;
mod authentication;
mod branch_replay;
mod event_ingestion;
mod response;
mod runtime_binding;
mod workflow;

pub use agent_binding::{
    AgentBindingQuery, claim_current_turn, get_agent_binding, get_agent_binding_current_turn,
    get_agent_binding_session_context,
};
pub use branch_replay::resolve_branch_replay;
pub use event_ingestion::{InternalEventRequest, InternalEventResponse, post_event};
pub use response::ApiError;
pub use runtime_binding::upsert_runtime_binding;
pub use workflow::{
    WorkflowPatchApplyRequest, WorkflowPatchBlockRequest, WorkflowPatchRequest, WorkflowRunRequest,
    WorkflowSubmissionRequest, apply_workflow_patch, block_workflow_patch, request_workflow_patch,
    run_workflow, submit_workflow_output,
};
