use axum::{
    Json,
    extract::{Path, State},
};
use pontia_application::{AppState, EventIngestService};
use serde::Deserialize;
use serde_json::{Value, json};

use super::response::ApiError;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnStartFailureRequest {
    runtime_instance_id: String,
    reason: TurnStartFailureReason,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TurnStartFailureReason {
    EventRejected,
    TransportFailed,
    MissingTurnId,
}

pub async fn report_turn_start_failure(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(request): Json<TurnStartFailureRequest>,
) -> Result<Json<Value>, ApiError> {
    let reason = match request.reason {
        TurnStartFailureReason::EventRejected => "event_rejected",
        TurnStartFailureReason::TransportFailed => "transport_failed",
        TurnStartFailureReason::MissingTurnId => "missing_turn_id",
    };
    EventIngestService::new(state.db())
        .with_agent_events(state.agent_events())
        .report_turn_start_failure(&session_id, &request.runtime_instance_id, reason)
        .await?;
    Ok(Json(json!({ "accepted": true })))
}
