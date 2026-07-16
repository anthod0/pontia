use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use serde_json::{Value, json};
use std::collections::HashMap;

use pontia_application::{
    AppState, TurnTimelineDirection, TurnTimelineService, TurnTimelineServiceError,
};

use super::common::{ApiResponse, ExternalApiError, authenticate, ok};

pub async fn get_turn_timeline(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let direction = match query.get("direction").map(String::as_str) {
        Some("forward") => TurnTimelineDirection::Forward,
        Some("backward") => TurnTimelineDirection::Backward,
        _ => {
            return Err(invalid_timeline_query(
                "direction must be forward or backward",
            ));
        }
    };
    let limit = query
        .get("limit")
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| invalid_timeline_query("limit must be an integer from 1 through 100"))?
        .unwrap_or(100);
    if !(1..=100).contains(&limit) {
        return Err(invalid_timeline_query(
            "limit must be an integer from 1 through 100",
        ));
    }
    let page = TurnTimelineService::new(state.db())
        .page(session_id, direction, query.get("turn_id").cloned(), limit)
        .await
        .map_err(turn_timeline_service_error)?;
    Ok(ok(json!({
        "session_id": page.session_id,
        "direction": page.direction,
        "items": page.items,
        "next_turn_id": page.next_turn_id,
    })))
}

fn invalid_timeline_query(message: impl Into<String>) -> ExternalApiError {
    ExternalApiError::custom(StatusCode::BAD_REQUEST, "invalid_timeline_query", message)
}

fn turn_timeline_service_error(error: TurnTimelineServiceError) -> ExternalApiError {
    match error {
        TurnTimelineServiceError::SessionNotFound => ExternalApiError::custom(
            StatusCode::NOT_FOUND,
            "session_not_found",
            "session not found",
        ),
        TurnTimelineServiceError::TurnNotFound => ExternalApiError::custom(
            StatusCode::NOT_FOUND,
            "turn_not_found",
            "timeline anchor Turn not found in Session",
        ),
        TurnTimelineServiceError::CapabilityUnavailable => ExternalApiError::custom(
            StatusCode::UNPROCESSABLE_ENTITY,
            "timeline_capability_unavailable",
            "Turn timeline capability is unavailable",
        ),
        TurnTimelineServiceError::TurnUnavailable { turn_id } => ExternalApiError::custom(
            StatusCode::CONFLICT,
            "turn_timeline_unavailable",
            format!("Turn {turn_id} has no available timeline range"),
        ),
        TurnTimelineServiceError::TimelineInvalid { turn_id } => ExternalApiError::custom(
            StatusCode::CONFLICT,
            "turn_timeline_invalid",
            format!("Turn {turn_id} has an invalid timeline range"),
        ),
        TurnTimelineServiceError::SourceUnavailable => ExternalApiError::custom(
            StatusCode::SERVICE_UNAVAILABLE,
            "timeline_source_unavailable",
            "timeline source is unavailable",
        ),
        TurnTimelineServiceError::Inner(error) => ExternalApiError::from(error),
    }
}
