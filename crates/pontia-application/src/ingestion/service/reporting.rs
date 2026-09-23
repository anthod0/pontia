use pontia_core::{
    Error,
    domain::{DomainEvent, EventType, MAX_TURN_INPUT_SUMMARY_CHARS, MAX_TURN_OUTPUT_SUMMARY_CHARS},
};
use serde_json::{Value, json};

use super::EventIngestService;
use crate::ingestion::{
    EventIngestResult, EventReportError, EventReportNormalizer, InternalEventValidationService,
    ReportedFact,
};

const MAX_EVENT_PAYLOAD_BYTES: usize = 64 * 1024;

impl EventIngestService {
    pub(crate) async fn report_native_fact(
        &self,
        session: &str,
        instance: &str,
        kind: EventType,
        mut data: Value,
    ) -> pontia_core::Result<()> {
        data["runtime_instance_id"] = json!(instance);
        self.report_fact(ReportedFact {
            session_id: session.into(),
            turn_id: None,
            fact_type: kind,
            data,
        })
        .await
        .map_err(|error| match error {
            crate::EventReportError::InvalidFact(message) => Error::Domain(message),
            crate::EventReportError::Ingestion(error) => error,
        })?;
        Ok(())
    }

    /// Processes a client fact, including validation and post-commit effects.
    ///
    /// This is the production entry point for all client adapters, whether called
    /// in-process or through a transport handler. It owns normalization, report
    /// validation and routing to durable ingestion or volatile notifications.
    /// Use [`crate::AppState::event_ingest_service`] to share the application's
    /// notification dependencies; the lower-level `ingest_*` methods do not replace
    /// this reporting contract.
    pub async fn report_fact(
        &self,
        fact: ReportedFact,
    ) -> Result<EventIngestResult, EventReportError> {
        // Low-level projection callers can construct a database-only service;
        // client reports require the complete set of shared effects.
        if self.effects.client_control.is_none()
            || self.effects.agent_events.is_none()
            || self.effects.live_output.is_none()
            || self.effects.volatile_events.is_none()
        {
            return Err(Error::InvalidConfig {
                key: "event_reporting",
                message: "event reporting requires control and shared notification dependencies"
                    .into(),
            }
            .into());
        }
        let failure_context = (fact.fact_type == EventType::TurnStarted)
            .then(|| fact.data.get("runtime_instance_id").and_then(Value::as_str))
            .flatten()
            .map(|runtime| (fact.session_id.clone(), runtime.to_owned()));
        let result = self.process_fact(fact).await;
        if let Err(error) = &result
            && error.is_permanent_rejection()
            && let Some((session_id, runtime_instance_id)) = failure_context
            && let Err(failure) = self
                .report_turn_start_failure(&session_id, &runtime_instance_id, "event_rejected")
                .await
        {
            tracing::warn!(%session_id, %failure, "could not record turn start reporting failure");
        }
        result
    }

    async fn process_fact(
        &self,
        fact: ReportedFact,
    ) -> Result<EventIngestResult, EventReportError> {
        if !fact.data.is_object() {
            return Err(invalid_fact("data must be a JSON object"));
        }
        let mut reported_event = EventReportNormalizer::new(self.pool.clone())
            .with_clients(self.clients.clone())
            .normalize(fact)
            .await
            .map_err(|error| invalid_fact(error.to_string()))?;
        if reported_event.event_type == EventType::TurnStarted {
            truncate_summary(
                &mut reported_event.payload,
                "/input/summary",
                MAX_TURN_INPUT_SUMMARY_CHARS,
            );
        }
        if reported_event.event_type == EventType::TurnOutput {
            truncate_summary(
                &mut reported_event.payload,
                "/output/summary",
                MAX_TURN_OUTPUT_SUMMARY_CHARS,
            );
        }
        if reported_event.event_type == EventType::SessionContextUsageUpdated {
            validate_context_usage_payload(&reported_event.payload)?;
        }
        let payload_size = serde_json::to_vec(&reported_event.payload)
            .map_err(Error::from)?
            .len();
        if payload_size > MAX_EVENT_PAYLOAD_BYTES {
            return Err(invalid_fact(format!(
                "payload exceeds maximum size of {MAX_EVENT_PAYLOAD_BYTES} bytes"
            )));
        }
        let event = DomainEvent::from(reported_event.clone());
        InternalEventValidationService::new()
            .with_clients(self.clients.clone())
            .validate(&event)
            .map_err(EventReportError::validation)?;
        self.ensure_confirmed_event_matches_session_boundary(&event)
            .await
            .map_err(EventReportError::validation)?;
        if event.event_type == EventType::SessionMessageUpdated {
            let state_version = self.volatile_state_version(&event.session_id).await?;
            self.effects
                .volatile_events
                .as_ref()
                .expect("validated reporting dependencies")
                .publish_debounced_session_message_updated(event.clone());
            return Ok(EventIngestResult {
                accepted: true,
                duplicate: false,
                event_id: event.event_id,
                session_id: event.session_id,
                turn_id: event.turn_id,
                state_version,
            });
        }
        Ok(self.ingest_confirmed_event(reported_event).await?)
    }
}

fn invalid_fact(message: impl Into<String>) -> EventReportError {
    EventReportError::InvalidFact(message.into())
}

fn truncate_summary(payload: &mut Value, pointer: &str, max_chars: usize) {
    if let Some(Value::String(summary)) = payload.pointer_mut(pointer) {
        *summary = summary.chars().take(max_chars).collect();
    }
}

fn validate_context_usage_payload(payload: &Value) -> Result<(), EventReportError> {
    let usage = payload
        .get("context_usage")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid_fact("payload.context_usage must be a JSON object"))?;

    for field in [
        "used_tokens",
        "max_tokens",
        "remaining_tokens",
        "input_tokens",
        "output_tokens",
        "cache_tokens",
    ] {
        if let Some(value) = usage.get(field)
            && !value.is_null()
            && value.as_u64().is_none()
        {
            return Err(invalid_fact(format!(
                "payload.context_usage.{field} must be a non-negative integer"
            )));
        }
    }

    if let Some(value) = usage.get("usage_ratio")
        && !value.is_null()
    {
        let ratio = value.as_f64().ok_or_else(|| {
            invalid_fact("payload.context_usage.usage_ratio must be between 0 and 1")
        })?;
        if !(0.0..=1.0).contains(&ratio) {
            return Err(invalid_fact(
                "payload.context_usage.usage_ratio must be between 0 and 1",
            ));
        }
    }

    if usage.contains_key("model") {
        return Err(invalid_fact(
            "payload.context_usage.model is not supported; use payload.model",
        ));
    }

    if let Some(value) = usage.get("confidence")
        && !value.is_null()
    {
        match value.as_str() {
            Some("exact" | "estimated" | "unknown") => {}
            _ => {
                return Err(invalid_fact(
                    "payload.context_usage.confidence must be exact, estimated, or unknown",
                ));
            }
        }
    }

    if let Some(value) = payload.get("model")
        && !value.is_null()
        && value.as_str().is_none()
    {
        return Err(invalid_fact("payload.model must be a string or null"));
    }

    Ok(())
}
