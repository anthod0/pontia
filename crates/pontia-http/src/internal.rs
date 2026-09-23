//! Internal HTTP API handlers and protocol types.
//!
//! The facade keeps router-facing paths stable while private responsibility
//! modules own each endpoint family and its policies.

mod authentication;
mod response;
mod workflow;

pub use response::ApiError;
pub use workflow::{
    WorkflowPatchApplyRequest, WorkflowPatchBlockRequest, WorkflowPatchRequest, WorkflowRunRequest,
    WorkflowSubmissionRequest, apply_workflow_patch, block_workflow_patch, request_workflow_patch,
    run_workflow, submit_workflow_output,
};
