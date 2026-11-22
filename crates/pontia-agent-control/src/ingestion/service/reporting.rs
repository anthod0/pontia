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
        data: Value,
    ) -> pontia_core::Result<()> {
        self.report_client_fact(session, Some(instance), kind, data)
            .await
    }

    pub async fn report_client_fact(
        &self,
        session: &str,
        runtime_id: Option<&str>,
        kind: EventType,
        mut data: Value,
    ) -> pontia_core::Result<()> {
        if let Some(runtime_id) = runtime_id {
            data["runtime_id"] = json!(runtime_id);
        }
        self.report_fact(ReportedFact {
            session_id: session.into(),
            turn_id: None,
            report_key: None,
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
    /// validation and routing to durable ingestion.
    /// Use [`crate::AgentControl::event_ingest_service`] to share the application's
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
        {
            return Err(Error::InvalidConfig {
                key: "event_reporting",
                message: "event reporting requires control and shared notification dependencies"
                    .into(),
            }
            .into());
        }
        self.process_fact(fact).await
    }

    async fn process_fact(
        &self,
        fact: ReportedFact,
    ) -> Result<EventIngestResult, EventReportError> {
        if !fact.data.is_object() {
            return Err(invalid_fact("data must be a JSON object"));
        }
        if fact
            .report_key
            .as_deref()
            .is_some_and(|report_key| report_key.trim().is_empty())
        {
            return Err(invalid_fact("report_key must not be empty"));
        }
        if fact.report_key.is_some()
            && fact.fact_type == EventType::TurnStarted
            && fact.turn_id.is_some()
        {
            return Err(invalid_fact(
                "reported turn.started cannot supply a canonical turn_id",
            ));
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
        if reported_event.event_type == EventType::SessionTitleUpdated
            && let Some(title) = reported_event.payload["title"].as_str()
        {
            let title = title.trim();
            reported_event.payload["title"] = if title.is_empty() {
                Value::Null
            } else {
                json!(title)
            };
        }
        if reported_event.event_type.is_compaction_event() {
            crate::ingestion::compaction::normalize(
                reported_event.event_type,
                &mut reported_event.payload,
            );
        }
        let event = DomainEvent::from(reported_event.clone());
        if let Some(result) = self.committed_report(&event).await? {
            return Ok(result);
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
        InternalEventValidationService::new()
            .with_clients(self.clients.clone())
            .validate(&event)
            .map_err(EventReportError::validation)?;
        if let Some(result) = self.validate_report_boundary(&event).await? {
            return Ok(result);
        }
        Ok(self.ingest_confirmed_event(reported_event).await?)
    }

    async fn validate_report_boundary(
        &self,
        event: &DomainEvent,
    ) -> Result<Option<EventIngestResult>, EventReportError> {
        match self
            .ensure_confirmed_event_matches_session_boundary(event)
            .await
        {
            Ok(()) => Ok(None),
            Err(error) => {
                // A concurrent commit can precede the binding change that invalidated this report.
                if let Some(result) = self.committed_report(event).await? {
                    Ok(Some(result))
                } else {
                    Err(EventReportError::validation(error))
                }
            }
        }
    }

    async fn committed_report(
        &self,
        event: &DomainEvent,
    ) -> pontia_core::Result<Option<EventIngestResult>> {
        if event.report_key.is_none() {
            return Ok(None);
        }
        let mut persisted = event.clone();
        if let Some(data) = self.clients.data(&event.client_type) {
            data.take_evidence(&mut persisted);
        }
        let mut connection = self.pool.acquire().await?;
        super::commit::duplicate_report(&mut connection, &persisted).await
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

#[cfg(test)]
mod tests {
    use super::*;
    use pontia_core::domain::{EventSource, ReportedEvent};
    use pontia_storage_sqlite::{connect_sqlite, run_migrations};

    #[tokio::test]
    async fn commit_between_key_lookup_and_boundary_rejection_is_acknowledged() {
        let db = connect_sqlite("sqlite::memory:").await.unwrap();
        run_migrations(&db).await.unwrap();
        let service = EventIngestService::for_projection_tests(db.clone());
        service
            .ingest_pontia_event(crate::PontiaEvent::new(
                "session",
                None,
                crate::PontiaEventSource::ExternalApi,
                "test",
                crate::PontiaEventType::SessionCreated,
                json!({}),
            ))
            .await
            .unwrap();
        sqlx::query("INSERT INTO session_runtimes(runtime_id,session_id,role,state,created_at) VALUES ('original','session','tui','running','2026-01-01')")
            .execute(&db).await.unwrap();
        let report = ReportedEvent::new(
            "first".into(),
            "session".into(),
            None,
            EventSource::AgentClient,
            "test".into(),
            EventType::SessionModelUpdated,
            json!({"runtime_id":"original","model":"model"}),
        )
        .with_report_key(Some("observation".into()));
        let replay = DomainEvent::from(report.clone());
        assert!(service.committed_report(&replay).await.unwrap().is_none());
        service.ingest_reported_event(report).await.unwrap();
        sqlx::query(
            "UPDATE session_runtimes SET runtime_id='replacement' WHERE session_id='session'",
        )
        .execute(&db)
        .await
        .unwrap();
        let result = service
            .validate_report_boundary(&replay)
            .await
            .unwrap()
            .unwrap();
        assert!(result.duplicate);
        assert_eq!(result.event_id, "first");
    }
}
