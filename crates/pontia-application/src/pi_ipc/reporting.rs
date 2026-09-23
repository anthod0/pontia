use super::Attach;
use crate::{AgentBindingService, AppState, EventReportError, ReportedFact};
use pontia_core::{Error, Result, domain::EventType};
use pontia_storage_sqlite::repositories::runtime_bindings::SqliteRuntimeBindingRepository;
use serde::Deserialize;
use serde_json::{Value, json};
use std::str::FromStr;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EventRequest {
    runtime_instance_id: String,
    event: Event,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Event {
    session_id: String,
    turn_id: Option<String>,
    #[serde(rename = "type")]
    fact_type: String,
    data: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StartFailure {
    client_session_key: String,
    session_id: String,
    runtime_instance_id: String,
    reason: FailureReason,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum FailureReason {
    EventRejected,
    TransportFailed,
    MissingTurnId,
}

async fn validate_identity(
    state: &AppState,
    identity: &Attach,
    session_id: &str,
    runtime_instance_id: &str,
) -> Result<()> {
    if identity.session_id != session_id || identity.runtime_instance_id != runtime_instance_id {
        return Err(Error::StateConflict(
            "Pi report does not match its connection identity".into(),
        ));
    }
    let current = SqliteRuntimeBindingRepository::new(state.db())
        .runtime_instance_id(session_id)
        .await?;
    if current.as_deref() != Some(runtime_instance_id) {
        return Err(Error::StateConflict(
            "Pi report runtime is no longer current".into(),
        ));
    }
    Ok(())
}

pub(super) async fn report_event(
    state: &AppState,
    identity: &Attach,
    params: Value,
) -> Result<Value> {
    let request: EventRequest = serde_json::from_value(params)?;
    let event = request.event;
    validate_identity(
        state,
        identity,
        &event.session_id,
        &request.runtime_instance_id,
    )
    .await?;
    let fact_type = EventType::from_str(&event.fact_type)?;
    let result = state
        .event_ingest_service()
        .report_fact(ReportedFact {
            session_id: event.session_id,
            turn_id: event.turn_id,
            fact_type,
            data: event.data,
        })
        .await
        .map_err(|error| match error {
            EventReportError::InvalidFact(message) => Error::Domain(message),
            EventReportError::Ingestion(error) => error,
        })?;
    Ok(json!({
        "accepted": result.accepted, "duplicate": result.duplicate,
        "event_id": result.event_id, "session_id": result.session_id,
        "turn_id": result.turn_id, "state_version": result.state_version,
    }))
}

pub(super) async fn start_failure(state: &AppState, params: Value) -> Result<Value> {
    let request: StartFailure = serde_json::from_value(params)?;
    // Failure acknowledgement can be retried after control has been invalidated.
    // Authenticate the native binding without attaching a control channel.
    let binding = AgentBindingService::new(state.db())
        .binding_for_client_session("pi", &request.client_session_key)
        .await?
        .ok_or_else(|| Error::NotFound("Pi binding not found".into()))?;
    if binding.session_id != request.session_id {
        return Err(Error::StateConflict(
            "Pi failure report does not match its native binding".into(),
        ));
    }
    let reason = match request.reason {
        FailureReason::EventRejected => "event_rejected",
        FailureReason::TransportFailed => "transport_failed",
        FailureReason::MissingTurnId => "missing_turn_id",
    };
    state
        .event_ingest_service()
        .report_turn_start_failure(&request.session_id, &request.runtime_instance_id, reason)
        .await?;
    Ok(json!({"accepted": true}))
}
