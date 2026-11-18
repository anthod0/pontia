use super::response::{ApiError, ApiResponse, ok};
use crate::HttpState;
use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use pontia_bot::{
    identity::TOOL_TOKEN_HEADER,
    user::{self, UserRequest},
    workspaces::{self, WorkspaceRequest},
};
use serde_json::{Value, json};

pub async fn update_user(
    State(state): State<HttpState>,
    headers: HeaderMap,
    request: Result<Json<UserRequest>, JsonRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let Json(request) = request.map_err(|error| ApiError::invalid_request(error.body_text()))?;
    authorize_tool(&state, &headers, &request.session_id).await?;
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
    headers: HeaderMap,
    request: Result<Json<WorkspaceRequest>, JsonRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let Json(request) = request.map_err(|error| ApiError::invalid_request(error.body_text()))?;
    authorize_tool(&state, &headers, &request.session_id).await?;
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

pub async fn session_messages(
    State(state): State<HttpState>,
    headers: HeaderMap,
    request: Result<Json<pontia_bot::session_messages::SessionMessagesRequest>, JsonRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let Json(request) = request.map_err(|error| ApiError::invalid_request(error.body_text()))?;
    authorize_tool(&state, &headers, &request.session_id).await?;
    let page = state
        .session_messages()
        .execute(request)
        .await
        .map_err(|error| match error {
            pontia_bot::session_messages::Error::Forbidden => ApiError::custom(
                StatusCode::FORBIDDEN,
                "forbidden",
                "Session is not authorized to read Bot Session messages",
            ),
            pontia_bot::session_messages::Error::Timeline(error) => {
                super::timeline::turn_timeline_service_error(error)
            }
        })?;
    Ok(ok(json!(page)))
}

pub(super) async fn authorize_tool(
    state: &HttpState,
    headers: &HeaderMap,
    session_id: &str,
) -> Result<(), ApiError> {
    let token = headers
        .get(TOOL_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok());
    state
        .bot_identity()
        .authenticate_tool(session_id, token)
        .await
        .map_err(|error| ApiError::custom(StatusCode::FORBIDDEN, "forbidden", error.to_string()))
}
