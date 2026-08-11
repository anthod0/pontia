use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use pontia_application::{AppState, ApprovalRegistrationRequest, ApprovalRegistrationService};
use serde_json::json;

use super::response::ApiError;

pub async fn post_claude_permission_request(
    State(state): State<AppState>,
    request: Result<Json<ApprovalRegistrationRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let Some(pending) = ApprovalRegistrationService::new(state.db(), state.approvals())
        .register(request)
        .await?
    else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    let request_event_id = pending.request_event_id.clone();
    let outcome = pending.wait().await;
    Ok(Json(json!({
        "data": {
            "result": outcome.response_value(),
            "request_event_id": request_event_id,
        }
    }))
    .into_response())
}
