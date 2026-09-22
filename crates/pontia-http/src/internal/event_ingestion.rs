use std::str::FromStr;

use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
};
use pontia_application::{AppState, ReportedFact};
use pontia_core::domain::EventType;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::response::ApiError;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InternalEventRequest {
    session_id: String,
    turn_id: Option<String>,
    #[serde(rename = "type")]
    fact_type: String,
    #[serde(alias = "payload")]
    data: Value,
}

#[derive(Debug, Serialize)]
pub struct InternalEventResponse {
    accepted: bool,
    duplicate: bool,
    event_id: String,
    session_id: String,
    turn_id: Option<String>,
    state_version: i64,
    warnings: Vec<String>,
}

pub async fn post_event(
    State(state): State<AppState>,
    request: Result<Json<InternalEventRequest>, JsonRejection>,
) -> Result<Json<InternalEventResponse>, ApiError> {
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let result = state
        .event_ingest_service()
        .report_fact(request.into_reported_fact()?)
        .await?;
    Ok(Json(InternalEventResponse {
        accepted: result.accepted,
        duplicate: result.duplicate,
        event_id: result.event_id,
        session_id: result.session_id,
        turn_id: result.turn_id,
        state_version: result.state_version,
        warnings: Vec::new(),
    }))
}

impl InternalEventRequest {
    fn into_reported_fact(self) -> Result<ReportedFact, ApiError> {
        let fact_type = EventType::from_str(&self.fact_type)
            .map_err(|err| ApiError::invalid_request(err.to_string()))?;
        Ok(ReportedFact {
            session_id: self.session_id,
            turn_id: self.turn_id,
            fact_type,
            data: self.data,
        })
    }
}
