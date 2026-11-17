use super::response::{ApiError, ApiResponse, ok};
use crate::HttpState;
use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::StatusCode,
};
use pontia_bot::{
    user::{self, UserRequest},
    workspaces::{self, WorkspaceRequest},
};
use serde_json::{Value, json};

pub async fn update_user(
    State(state): State<HttpState>,
    request: Result<Json<UserRequest>, JsonRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let Json(request) = request.map_err(|error| ApiError::invalid_request(error.body_text()))?;
    state
        .user_commands()
        .execute(request)
        .await
        .map_err(|error| match error {
            user::Error::Forbidden(_) => {
                ApiError::custom(StatusCode::FORBIDDEN, "forbidden", error.to_string())
            }
            user::Error::Rejected { .. } => ApiError::custom(
                StatusCode::CONFLICT,
                "user_update_rejected",
                error.to_string(),
            ),
            _ => ApiError::custom(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                error.to_string(),
            ),
        })?;
    Ok(ok(json!({"updated": true})))
}

pub async fn workspace_command(
    State(state): State<HttpState>,
    request: Result<Json<WorkspaceRequest>, JsonRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let Json(request) = request.map_err(|error| ApiError::invalid_request(error.body_text()))?;
    let workspace =
        state
            .workspace_commands()
            .execute(request)
            .await
            .map_err(|error| match error {
                workspaces::Error::Forbidden(_) => {
                    ApiError::custom(StatusCode::FORBIDDEN, "forbidden", error.to_string())
                }
                workspaces::Error::Control(error) => error.into(),
            })?;
    Ok(ok(json!({"workspace": workspace})))
}
