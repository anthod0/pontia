//! Internal HTTP API handlers and protocol types.
//!
//! The facade keeps router-facing paths stable while private responsibility
//! modules own each endpoint family and its policies.

mod agent_binding;
mod authentication;
mod response;
mod workflow;

pub use agent_binding::{AgentBindingQuery, get_agent_binding, get_agent_binding_current_turn};
pub use response::ApiError;
pub use workflow::{
    WorkflowPatchApplyRequest, WorkflowPatchBlockRequest, WorkflowPatchRequest, WorkflowRunRequest,
    WorkflowSubmissionRequest, apply_workflow_patch, block_workflow_patch, request_workflow_patch,
    run_workflow, submit_workflow_output,
};
