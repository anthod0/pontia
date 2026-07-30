use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use pontia_application::{AppState, ApprovalCommandService, ApprovalDecisionRequest};
use serde_json::json;

use super::common::{ExternalApiError, authenticate, idempotent, ok};

pub async fn decide_approval(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((session_id, request_event_id)): Path<(String, String)>,
    Json(request): Json<ApprovalDecisionRequest>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ApprovalCommandService::new(state.db(), state.approvals());
    let operation = format!("decide_approval:{session_id}:{request_event_id}");
    let action_session_id = session_id.clone();
    let action_request_event_id = request_event_id.clone();
    let outcome = idempotent(&state, &headers, operation, || async move {
        service
            .decide(&action_session_id, &action_request_event_id, request)
            .await
    })
    .await?;
    Ok((
        StatusCode::OK,
        ok(json!({
            "request_event_id": request_event_id,
            "delivered": outcome.data["delivered"],
        })),
    )
        .into_response())
}
