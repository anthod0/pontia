use pontia_agent_control::ipc::identity::NativeAttachment;
use pontia_agent_control::{ClientServiceContext, EventReportError, ReportedFact};
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
    report_key: String,
    #[serde(rename = "type")]
    fact_type: String,
    data: Value,
}

pub(super) async fn report_event(
    state: &ClientServiceContext,
    identity: &NativeAttachment,
    params: Value,
) -> Result<Value> {
    let request: EventRequest = serde_json::from_value(params)?;
    let mut event = request.event;
    if identity.session_id != event.session_id || identity.runtime_id != request.runtime_id {
        return Err(Error::StateConflict(
            "Pi report does not match its connection identity".into(),
        ));
    }
    if event.report_key.trim().is_empty() {
        return Err(Error::Domain(
            "Pi event requires a non-empty report_key".into(),
        ));
    }
    let data = event
        .data
        .as_object_mut()
        .ok_or_else(|| Error::Domain("Pi fact data must be an object".into()))?;
    data.entry("runtime_id")
        .or_insert(json!(request.runtime_id));
    let fact_type = EventType::from_str(&event.fact_type)?;
    let fact = ReportedFact {
        session_id: event.session_id,
        turn_id: event.turn_id,
        report_key: Some(event.report_key),
        fact_type,
        data: event.data,
    };
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
