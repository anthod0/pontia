//! Seed client facts through the shared application service in HTTP scenarios.
use axum::http::StatusCode;
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
        Err(error) => error_response(error),
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

fn error_response(error: EventReportError) -> (StatusCode, Value) {
    use pontia_core::Error;

    let (status, code, message) = match error {
        EventReportError::InvalidFact(message) => {
            (StatusCode::BAD_REQUEST, "invalid_request", message)
        }
        EventReportError::Ingestion(Error::Domain(message) | Error::StateConflict(message)) => {
            (StatusCode::CONFLICT, "state_conflict", message)
        }
        EventReportError::Ingestion(Error::NotFound(message)) => {
            (StatusCode::NOT_FOUND, "not_found", message)
        }
        EventReportError::Ingestion(Error::CapabilityUnavailable(message)) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "capability_unavailable",
            message,
        ),
        other => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            other.to_string(),
        ),
    };
    (
        status,
        json!({ "error": { "code": code, "message": message } }),
    )
}
