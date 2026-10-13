use super::Attach;
use pontia_application::{AppState, EventReportError, ReportedFact};
use pontia_core::{Error, Result, domain::EventType};
use serde::Deserialize;
use serde_json::{Value, json};
use std::str::FromStr;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EventRequest {
    runtime_id: String,
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
    runtime_id: String,
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
    runtime_id: &str,
) -> Result<()> {
    if identity.session_id != session_id || identity.runtime_id != runtime_id {
        return Err(Error::StateConflict(
            "Pi report does not match its connection identity".into(),
        ));
    }
    state
        .client_control()
        .validate_runtime(session_id, runtime_id)
        .await
}

pub(super) async fn report_event(
    state: &AppState,
    identity: &Attach,
    params: Value,
) -> Result<Value> {
    let request: EventRequest = serde_json::from_value(params)?;
    let mut event = request.event;
    if event.data["runtime_id"]
        .as_str()
        .is_some_and(|id| id != request.runtime_id)
    {
        return Err(Error::StateConflict(
            "Pi fact payload has a different runtime identity".into(),
        ));
    }
    event.data["runtime_id"] = json!(request.runtime_id);
    validate_identity(state, identity, &event.session_id, &request.runtime_id).await?;
    let fact_type = EventType::from_str(&event.fact_type)?;
    let fact = ReportedFact {
        session_id: event.session_id,
        turn_id: event.turn_id,
        fact_type,
        data: event.data,
    };
    // dispatch holds the shared connection identity gate through ingestion.
    // Replayed hook facts therefore cannot race either the original report or
    // replacement connections after a lost acknowledgement / extension reload.
    if fact_type.is_compaction_event() {
        let queries = state.queries();
        if let Some(existing) = queries
            .list_session_events(&fact.session_id)
            .await?
            .into_iter()
            .find(|existing| {
                existing.event_type == event.fact_type
                    && existing.payload["operation_id"] == fact.data["operation_id"]
            })
        {
            // A replay must match the previously validated canonical payload.
            // Missing optional null fields are equivalent; extra fields or a
            // changed required value are not. Fresh facts use report_fact below.
            if fact.turn_id.is_some()
                || existing
                    .payload
                    .as_object()
                    .unwrap()
                    .iter()
                    .any(|(key, value)| fact.data.get(key).unwrap_or(&Value::Null) != value)
                || fact
                    .data
                    .as_object()
                    .unwrap()
                    .iter()
                    .any(|(key, value)| existing.payload.get(key) != Some(value))
            {
                return Err(Error::Domain(
                    "Pi compaction replay changed its metadata".into(),
                ));
            }
            let version = queries
                .resolve_event_cursor(
                    pontia_application::views::events::EventStreamScope::Session {
                        session_id: &fact.session_id,
                    },
                    &existing.event_id,
                )
                .await?;
            return Ok(
                json!({"accepted":true,"duplicate":true,"event_id":existing.event_id,
                "session_id":fact.session_id,"turn_id":null,"state_version":version}),
            );
        }
    }
    let result = state
        .event_ingest_service()
        .report_fact(fact)
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

pub(super) async fn start_failure(
    state: &AppState,
    identity: &Attach,
    params: Value,
) -> Result<Value> {
    let request: StartFailure = serde_json::from_value(params)?;
    validate_identity(state, identity, &request.session_id, &request.runtime_id).await?;
    state
        .client_control()
        .validate_identity(
            "pi",
            &request.client_session_key,
            &request.session_id,
            &request.runtime_id,
        )
        .await?;
    let reason = match request.reason {
        FailureReason::EventRejected => "event_rejected",
        FailureReason::TransportFailed => "transport_failed",
        FailureReason::MissingTurnId => "missing_turn_id",
    };
    state
        .event_ingest_service()
        .report_turn_start_failure(&request.session_id, &request.runtime_id, reason)
        .await?;
    Ok(json!({"accepted": true}))
}
