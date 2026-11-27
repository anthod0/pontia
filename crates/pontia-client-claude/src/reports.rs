use crate::{
    Adapter,
    peer::{Peer, State, field, invalid},
};
use pontia_agent_control::{
    EventIngestService, EventReportError, ReportedFact, client_contract::ClientEventInterpreter,
};
use pontia_core::{
    Error, Result,
    domain::{DomainEvent, EventSource, EventType, TurnState},
};
use serde_json::{Value, json};

impl ClientEventInterpreter for Adapter {
    fn accompanying_runtime_event(&self, event: &DomainEvent) -> Result<Option<DomainEvent>> {
        if event.event_type != EventType::SessionReady {
            return Ok(None);
        }
        Ok(Some(DomainEvent {
            event_id: format!("{}:runtime", event.event_id),
            event_type: EventType::RuntimeReady,
            report_key: None,
            source: EventSource::RuntimeManager,
            payload: json!({
                "session_event_id": event.event_id,
                "runtime_id": event.payload["runtime_id"],
                "role": "claude_sdk_host",
            }),
            ..event.clone()
        }))
    }
}

pub(crate) struct HostReport<'a> {
    kind: EventType,
    key: &'a str,
    event: &'a Value,
}

impl<'a> HostReport<'a> {
    pub(crate) fn parse(host: &str, params: &'a Value) -> Result<Option<Self>> {
        let event = &params["event"];
        let kind = match event["kind"].as_str() {
            Some("started") => EventType::TurnStarted,
            Some("finished") => match event["outcome"].as_str() {
                Some("completed") => EventType::TurnCompleted,
                Some("failed") => EventType::TurnFailed,
                Some("interrupted") => EventType::TurnInterrupted,
                _ => return Err(invalid()),
            },
            Some("ready") => EventType::SessionReady,
            Some("model") => EventType::SessionModelUpdated,
            Some("reasoning_effort") => EventType::SessionReasoningEffortUpdated,
            Some("context_usage") => EventType::SessionContextUsageUpdated,
            Some("compaction") => match event["phase"].as_str() {
                Some("started") => EventType::SessionCompactionStarted,
                Some("completed") => EventType::SessionCompactionCompleted,
                Some("failed") => EventType::SessionCompactionFailed,
                _ => return Err(invalid()),
            },
            _ => return Ok(None),
        };
        let key = field(params, "report_key")?;
        let suffix = match kind {
            EventType::TurnStarted => format!("{}:start", field(event, "input_id")?),
            EventType::TurnCompleted | EventType::TurnFailed | EventType::TurnInterrupted => {
                field(event, "result_id")?.into()
            }
            _ => params["sequence"].as_u64().ok_or_else(invalid)?.to_string(),
        };
        if key != format!("claude:{host}:{suffix}") {
            return Err(Error::StateConflict(
                "Claude host report key does not match observation".into(),
            ));
        }
        Ok(Some(Self { kind, key, event }))
    }
}

impl Peer {
    pub(crate) async fn report(
        &self,
        events: &EventIngestService,
        state: &mut State,
        host: &str,
        report: HostReport<'_>,
        stale: bool,
    ) -> Result<Value> {
        let HostReport { kind, key, event } = report;
        // The host identifies input and result messages. Pontia supplied the input
        // summary, inbox association and canonical Turn, which can outlive this Peer.
        let existing: Option<Option<String>> = sqlx::query_scalar(
            "SELECT turn_id FROM events
             WHERE session_id=? AND event_type=? AND report_key=?",
        )
        .bind(&self.session)
        .bind(kind.to_string())
        .bind(key)
        .fetch_optional(&events.db())
        .await?;
        let mut replay = existing.is_some();
        let mut turn_id = None;
        let mut data = match kind {
            EventType::SessionReady => {
                let native = field(event, "native_session_id")?;
                if existing.is_none()
                    && (state.native.as_deref().is_some_and(|id| id != native)
                        || self.resume.as_deref().is_some_and(|id| id != native))
                {
                    return Err(invalid());
                }
                json!({"client_session_key":native, "launch_cwd":self.cwd, "binding_metadata":{"claude_config_dir":self.config_dir,"launch_model":self.model}})
            }
            EventType::TurnStarted => {
                let id = field(event, "input_id")?;
                let input = &event["input"];
                if !input["summary"].is_string() {
                    return Err(invalid());
                }
                if !replay
                    && state
                        .input
                        .as_ref()
                        .is_some_and(|i| i.id != id || i.turn.is_some())
                {
                    return Err(invalid());
                }
                json!({"claude_input_id":id, "claude_history_anchor":id, "input":input})
            }
            EventType::TurnCompleted | EventType::TurnFailed | EventType::TurnInterrupted => {
                let id = field(event, "input_id")?;
                turn_id = if let Some(turn) = &existing {
                    turn.clone()
                } else if let Some(input) = state.input.as_ref().filter(|i| i.id == id) {
                    Some(input.turn.clone().ok_or_else(invalid)?)
                } else {
                    // A later result for an already finished input belongs to the
                    // original Turn, even when a different input is now active.
                    let original: Option<(String, TurnState)> = sqlx::query_as(
                        "SELECT t.turn_id,t.state FROM events e JOIN turns t ON t.turn_id=e.turn_id
                         WHERE e.session_id=? AND e.event_type='turn.started' AND e.report_key=?",
                    )
                    .bind(&self.session)
                    .bind(format!("claude:{host}:{id}:start"))
                    .fetch_optional(&events.db())
                    .await?;
                    replay = true;
                    Some(
                        original
                            .filter(|(_, state)| state.is_terminal())
                            .ok_or_else(invalid)?
                            .0,
                    )
                };
                json!({"claude_input_id":id,"claude_result_id":event["result_id"],"claude_history_anchor":event["history_anchor"]})
            }
            _ => {
                if existing.is_none() && state.native.is_none() {
                    return Err(invalid());
                }
                match kind {
                    EventType::SessionModelUpdated => json!({"model":field(event,"model")?}),
                    EventType::SessionReasoningEffortUpdated => {
                        json!({"reasoning_effort":event.get("reasoning_effort").ok_or_else(invalid)?})
                    }
                    EventType::SessionContextUsageUpdated => {
                        json!({"context_usage":event["context_usage"]})
                    }
                    _ => event["data"]
                        .as_object()
                        .cloned()
                        .map(Value::Object)
                        .ok_or_else(invalid)?,
                }
            }
        };
        if stale && !replay {
            return Err(Error::StateConflict(
                "Claude host sequence already processed".into(),
            ));
        }

        data["runtime_id"] = json!(self.runtime);
        let ack = events
            .report_fact(ReportedFact {
                session_id: self.session.clone(),
                turn_id,
                report_key: Some(key.into()),
                fact_type: kind,
                data,
            })
            .await
            .map_err(report_error)?;
        match kind {
            EventType::SessionReady if state.native.is_none() => {
                state.native = Some(field(event, "native_session_id")?.into());
            }
            EventType::TurnStarted => {
                if state.input.is_none() && !replay {
                    state.input = Some(crate::peer::Input {
                        id: field(event, "input_id")?.into(),
                        turn: ack.turn_id.clone(),
                        sequence: 0,
                    });
                }
                if let Some(input) = state
                    .input
                    .as_mut()
                    .filter(|input| event["input_id"] == input.id)
                {
                    input.turn = ack.turn_id.clone();
                }
            }
            EventType::TurnCompleted | EventType::TurnFailed | EventType::TurnInterrupted
                if state.input.as_ref().is_some_and(|input| {
                    event["input_id"] == input.id && input.turn == ack.turn_id
                }) =>
            {
                state.input = None;
                if !ack.duplicate
                    && let Some(writer) = events.timeline_cache_writer()
                {
                    let session = self.session.clone();
                    let (pending, writing) = tokio::sync::watch::channel(());
                    state.cache_write = Some(pending);
                    tokio::spawn(async move {
                        writer.flush_native_ranges(&session).await;
                        drop(writing);
                    });
                }
            }
            _ => {}
        }
        Ok(
            json!({"accepted":ack.accepted, "duplicate":ack.duplicate, "event_id":ack.event_id,
            "session_id":ack.session_id, "turn_id":ack.turn_id, "state_version":ack.state_version}),
        )
    }
}

pub(crate) fn report_error(error: EventReportError) -> Error {
    match error {
        EventReportError::InvalidFact(message) => Error::Domain(message),
        EventReportError::Ingestion(error) => error,
    }
}
