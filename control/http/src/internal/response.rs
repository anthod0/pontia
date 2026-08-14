use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use pontia_core::error::Error;
use serde_json::json;

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    pub(crate) fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_request",
            message: message.into(),
        }
    }

    pub(super) fn authentication_failed(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "authentication_failed",
            message: message.into(),
        }
    }

    pub(super) fn from_workflow(error: pontia_workflow::Error) -> Self {
        use pontia_workflow::Error as WorkflowError;

        match error {
            WorkflowError::Pontia(error) => Self::from(error),
            WorkflowError::WorkflowNotFound(workflow_id) => Self {
                status: StatusCode::NOT_FOUND,
                code: "not_found",
                message: format!("workflow {workflow_id} not found"),
            },
            WorkflowError::NodeForSessionNotFound(session_id) => Self {
                status: StatusCode::NOT_FOUND,
                code: "not_found",
                message: format!("session {session_id} is not bound to a workflow Agent Node"),
            },
            WorkflowError::InvalidDefinition(message) => Self::invalid_request(message),
            WorkflowError::UnsupportedNodeType(node_type) => {
                Self::invalid_request(format!("unsupported Workflow Node type: {node_type}"))
            }
            WorkflowError::InvalidWorkflowId(workflow_id) => {
                Self::invalid_request(format!("invalid Workflow ID: {workflow_id}"))
            }
            WorkflowError::InvalidHandoffFileName(message) => {
                Self::invalid_request(format!("invalid Handoff file name: {message}"))
            }
            WorkflowError::WorkflowNotRunning { .. }
            | WorkflowError::RuntimeMismatch { .. }
            | WorkflowError::OutputMismatch { .. } => Self {
                status: StatusCode::CONFLICT,
                code: "state_conflict",
                message: error.to_string(),
            },
            WorkflowError::RootNodeNotFound(_)
            | WorkflowError::InvalidObservation(_)
            | WorkflowError::MissingCreatedSessionId
            | WorkflowError::RuntimeControlUnavailable { .. }
            | WorkflowError::Io(_)
            | WorkflowError::Json(_) => Self {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "internal_error",
                message: error.to_string(),
            },
        }
    }
}

impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        match error {
            Error::Domain(message) | Error::StateConflict(message) => Self {
                status: StatusCode::CONFLICT,
                code: "state_conflict",
                message,
            },
            Error::NotFound(message) => Self {
                status: StatusCode::NOT_FOUND,
                code: "not_found",
                message,
            },
            Error::CapabilityUnavailable(message) => Self {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "capability_unavailable",
                message,
            },
            other => Self {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "internal_error",
                message: other.to_string(),
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(json!({
            "error": {
                "code": self.code,
                "message": self.message,
            }
        }));
        (self.status, body).into_response()
    }
}
