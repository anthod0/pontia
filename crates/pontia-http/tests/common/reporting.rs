//! Seed client facts through the shared application service in HTTP scenarios.
use axum::{http::StatusCode, response::IntoResponse};
use http_body_util::BodyExt;
use pontia_application::{AppState, EventIngestResult, EventReportError, ReportedFact};
use pontia_core::domain::EventType;
use serde_json::{Value, json};
use std::str::FromStr;

pub(crate) async fn report_fact_result(
    state: AppState,
    body: Value,
) -> Result<EventIngestResult, EventReportError> {
    let fact = ReportedFact {
        session_id: body["session_id"].as_str().unwrap().into(),
        turn_id: body["turn_id"].as_str().map(str::to_owned),
        fact_type: EventType::from_str(body["type"].as_str().unwrap()).unwrap(),
        data: body["data"].clone(),
    };
    state.event_ingest_service().report_fact(fact).await
}

pub(crate) async fn report_fact(state: AppState, body: Value) -> (StatusCode, Value) {
    match report_fact_result(state, body).await {
        Ok(result) => (
            StatusCode::OK,
            json!({
                "accepted": result.accepted, "duplicate": result.duplicate,
                "event_id": result.event_id, "session_id": result.session_id,
                "turn_id": result.turn_id, "state_version": result.state_version,
            }),
        ),
        Err(error) => error_response(error.into()).await,
    }
}

pub(crate) async fn report_start_failure(
    state: AppState,
    session: &str,
    body: Value,
) -> pontia_core::Result<()> {
    state
        .event_ingest_service()
        .report_turn_start_failure(
            session,
            body["runtime_instance_id"].as_str().unwrap(),
            body["reason"].as_str().unwrap(),
        )
        .await
}

async fn error_response(error: pontia_http::internal::ApiError) -> (StatusCode, Value) {
    let response = error.into_response();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
