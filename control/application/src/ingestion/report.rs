use serde_json::{Value, json};
use sqlx::SqlitePool;

use pontia_core::{
    domain::{EventSource, EventType, ReportedEvent},
    error::{Error, Result},
    ids::{new_event_id, new_turn_id},
};
use pontia_storage_sqlite::repositories::{
    sessions::SqliteSessionRepository, turns::SqliteTurnRepository,
};

/// A client-observed fact at the Internal Event API boundary.
///
/// Pontia identity, source, client type, canonical time and canonical payload are
/// deliberately absent: those belong to normalization, not to the client.
#[derive(Debug, Clone)]
pub struct ReportedFact {
    pub session_id: String,
    pub turn_id: Option<String>,
    pub fact_type: EventType,
    pub data: Value,
}

#[derive(Clone)]
pub struct EventReportNormalizer {
    pool: SqlitePool,
}

impl EventReportNormalizer {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn normalize(&self, fact: ReportedFact) -> Result<ReportedEvent> {
        if !fact.fact_type.is_client_reportable() {
            return Err(Error::Domain(format!(
                "{} is owned by the Pontia control plane and cannot be reported by an agent client",
                fact.fact_type
            )));
        }

        let session = SqliteSessionRepository::new(self.pool.clone())
            .get_session(&fact.session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {} not found", fact.session_id)))?;

        if event_type_can_create_turn(fact.fact_type)
            && let Some(turn_id) = fact.turn_id.as_deref()
        {
            let owning_session_id = SqliteTurnRepository::new(self.pool.clone())
                .turn_session_id(turn_id)
                .await?
                .ok_or_else(|| {
                    Error::Domain(format!(
                        "event {} cannot create client-supplied turn {turn_id}",
                        fact.fact_type
                    ))
                })?;
            if owning_session_id != fact.session_id {
                return Err(Error::Domain(format!(
                    "turn {turn_id} belongs to session {owning_session_id}, not {}",
                    fact.session_id
                )));
            }
        }

        let turn_id = match (fact.fact_type, fact.turn_id) {
            (EventType::TurnStarted, None) => Some(new_turn_id().to_string()),
            (event_type, None) if event_type.requires_turn_id() => {
                return Err(Error::Domain(format!(
                    "event {event_type} requires turn_id"
                )));
            }
            (_, turn_id) => turn_id,
        };
        let source = if fact.fact_type.is_turn_event() {
            EventSource::AgentAdapter
        } else {
            EventSource::AgentClient
        };
        let payload = normalize_payload(&session.client_type, fact.fact_type, fact.data)?;

        Ok(ReportedEvent::new(
            new_event_id().to_string(),
            fact.session_id,
            turn_id,
            source,
            session.client_type,
            fact.fact_type,
            payload,
        ))
    }
}

fn event_type_can_create_turn(event_type: EventType) -> bool {
    matches!(
        event_type,
        EventType::TurnCreated | EventType::TurnQueued | EventType::TurnStarted
    )
}

fn normalize_payload(client_type: &str, event_type: EventType, data: Value) -> Result<Value> {
    let object = data
        .as_object()
        .ok_or_else(|| Error::Domain("data must be a JSON object".to_string()))?;
    if client_type != "pi" {
        return Ok(data);
    }

    let payload = match event_type {
        EventType::TurnStarted => {
            let input_summary = object
                .get("input_summary")
                .or_else(|| data.pointer("/input/summary"))
                .cloned()
                .unwrap_or(Value::Null);
            let previous_leaf_id = object
                .get("previous_leaf_id")
                .or_else(|| data.pointer("/timeline_anchor/previous_leaf_id"))
                .cloned()
                .unwrap_or(Value::Null);
            let mut payload = json!({
                "runtime_instance_id": object.get("runtime_instance_id").cloned().unwrap_or(Value::Null),
                "input": { "summary": input_summary },
                "timeline_anchor": { "previous_leaf_id": previous_leaf_id },
            });
            if let Some(inbox_message_id) = object
                .get("inbox_message_id")
                .or_else(|| data.pointer("/metadata/inbox_message_id"))
            {
                payload["metadata"] = json!({ "inbox_message_id": inbox_message_id });
            }
            if let Some(topology_context) = object.get("topology_context") {
                payload["topology_context"] = topology_context.clone();
            }
            payload
        }
        EventType::TurnOutput => json!({
            "output": {
                "summary": object
                    .get("output_summary")
                    .or_else(|| data.pointer("/output/summary"))
                    .cloned()
                    .unwrap_or(Value::Null),
            }
        }),
        EventType::TurnCompleted => json!({
            "timeline_anchor": {
                "terminal_leaf_id": object
                    .get("terminal_leaf_id")
                    .or_else(|| data.pointer("/timeline_anchor/terminal_leaf_id"))
                    .cloned()
                    .unwrap_or(Value::Null),
            }
        }),
        EventType::TurnFailed => json!({
            "failure": {
                "message": object
                    .get("failure_message")
                    .or_else(|| data.pointer("/failure/message"))
                    .cloned()
                    .unwrap_or(Value::Null),
            },
            "timeline_anchor": {
                "terminal_leaf_id": object
                    .get("terminal_leaf_id")
                    .or_else(|| data.pointer("/timeline_anchor/terminal_leaf_id"))
                    .cloned()
                    .unwrap_or(Value::Null),
            }
        }),
        _ => data,
    };
    Ok(payload)
}
