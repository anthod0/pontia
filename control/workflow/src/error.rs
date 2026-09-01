pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Pontia(#[from] pontia_core::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    TomlSerialization(#[from] toml::ser::Error),

    #[error("invalid Workflow definition: {0}")]
    InvalidDefinition(String),

    #[error("unsupported Workflow Node type: {0}")]
    UnsupportedNodeType(String),

    #[error("invalid Workflow ID: {0}")]
    InvalidWorkflowId(String),

    #[error("workflow {0} not found")]
    WorkflowNotFound(String),

    #[error("workflow {0} has no root node")]
    RootNodeNotFound(String),

    #[error("workflow {0} cannot be observed because its definition is invalid")]
    InvalidObservation(String),

    #[error("session creation response did not contain a session_id")]
    MissingCreatedSessionId,

    #[error("invalid Handoff file name: {0}")]
    InvalidHandoffFileName(String),

    #[error("session {0} is not bound to a workflow Agent Node")]
    NodeForSessionNotFound(String),

    #[error("workflow {workflow_id} must be running, but is {state}")]
    WorkflowNotRunning { workflow_id: String, state: String },

    #[error("runtime {runtime_instance_id} is not the current runtime for session {session_id}")]
    RuntimeMismatch {
        session_id: String,
        runtime_instance_id: String,
    },

    #[error("runtime control is unavailable for session {session_id}: {message}")]
    RuntimeControlUnavailable { session_id: String, message: String },

    #[error("output {actual} does not match Agent Node declared output {expected}")]
    OutputMismatch { expected: String, actual: String },
}
