use axum::{
    Json,
    extract::{Path, State, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    application::{AgentToolRequest, AgentToolService, AppState},
    error::Error,
};

#[derive(Debug, Serialize)]
pub struct InternalAgentToolResponse {
    ok: bool,
    tool: String,
    result: Value,
}

pub async fn post_agent_tool(
    State(state): State<AppState>,
    Path(tool_name): Path<String>,
    request: Result<Json<AgentToolRequest>, JsonRejection>,
) -> Result<Json<InternalAgentToolResponse>, ApiError> {
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let result = AgentToolService::with_graph(state.db, state.graph)
        .call(&tool_name, request)
        .await?;
    Ok(Json(InternalAgentToolResponse {
        ok: true,
        tool: tool_name,
        result: serde_json::to_value(result)?,
    }))
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_request",
            message: message.into(),
        }
    }
}

impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        match error {
            Error::Domain(message) => Self {
                status: StatusCode::BAD_REQUEST,
                code: "invalid_request",
                message,
            },
            Error::StateConflict(message) => Self {
                status: StatusCode::CONFLICT,
                code: "state_conflict",
                message,
            },
            Error::NotFound(message) => Self {
                status: StatusCode::NOT_FOUND,
                code: "not_found",
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

impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            message: error.to_string(),
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
