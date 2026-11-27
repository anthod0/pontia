use axum::{
    Json,
    extract::{Path, State},
};
use pontia_agent_control::SubmitBranchReplayRequest;
use pontia_application::AppState;
use serde_json::{Value, json};

use super::response::{ApiError, ApiResponse, ok};

pub async fn submit_branch_replay(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(request): Json<SubmitBranchReplayRequest>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let outcome = state
        .control()
        .branch_replays()
        .submit(&session_id, &request.target_turn_id, &request.input)
        .await?;
    Ok(ok(json!({ "branch_replay": outcome })))
}
