use crate::{
    EventIngestService, InboxCommandService, PontiaEvent, PontiaEventSource, PontiaEventType,
    ReportedFact, UpsertAgentBindingRequest,
};
use pontia_core::{Error, Result, domain::EventType, ids::new_session_id};
use pontia_runtime::RuntimeStartResult;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use std::path::Path;

/// Applies client observations using the same persistent identity and ingestion rules as other entrypoints.
#[derive(Clone)]
pub struct NativeSessionService {
    events: EventIngestService,
    pool: SqlitePool,
}

pub struct NativeSessionIdentity {
    pub launch_cwd: String,
    pub client_session_file: Option<String>,
}

pub struct NativeSessionObservation {
    pub identity: NativeSessionIdentity,
    pub provisioned_runtime: RuntimeStartResult,
    pub instance_id: String,
    pub capabilities: crate::views::SessionCapabilities,
    pub details: Value,
}

pub struct NativeTurnObservation {
    pub native_turn_id: String,
    pub input_summary: Option<String>,
    pub output_summary: Option<String>,
    pub terminal: Result<Option<EventType>>,
    pub started_at: Value,
    pub completed_at: Value,
    pub failure: Option<String>,
    pub origin: String,
}

impl NativeSessionService {
    pub fn new(events: EventIngestService) -> Self {
        Self {
            pool: events.db(),
            events,
        }
    }

    pub async fn provision(&self, session: &str, runtime: &RuntimeStartResult) -> Result<()> {
        pontia_storage_sqlite::repositories::runtime_bindings::SqliteRuntimeBindingRepository::new(
            self.pool.clone(),
        )
        .upsert_binding(crate::sessions::runtime_binding_record(session, runtime)?)
        .await
    }

    pub async fn confirm(
        &self,
        binding: UpsertAgentBindingRequest,
        instance: &str,
        expected_instance: Option<&str>,
        capabilities: &crate::views::SessionCapabilities,
        details: Value,
    ) -> Result<()> {
        let session = binding.session_id.clone();
        let client = binding.client_type.clone();
        let mut tx = self.pool.begin().await?;
        let updated = sqlx::query("UPDATE runtime_bindings SET runtime_instance_id=?, binding_state='confirmed', capabilities=?, adapter_details=json_set(adapter_details,?,json(?)) WHERE session_id=? AND runtime_instance_id IS ?")
            .bind(instance).bind(serde_json::to_string(capabilities)?).bind(format!("$.{client}")).bind(details.to_string()).bind(&session).bind(expected_instance).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            return Err(Error::StateConflict(
                "Runtime binding changed before confirmation".into(),
            ));
        }
        crate::agent_bindings::upsert_agent_binding_in_tx(&mut tx, binding).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn resolve_observed_session(
        &self,
        client: &str,
        native_key: &str,
        observation: Result<NativeSessionObservation>,
    ) -> Result<String> {
        if let Some(binding) = crate::AgentBindingService::new(self.pool.clone())
            .binding_for_client_session(client, native_key)
            .await?
        {
            return Ok(binding.session_id);
        }
        let observation = observation?;
        let identity = observation.identity;
        let session = self.observed_session(client, &identity.launch_cwd).await?;
        self.provision(&session, &observation.provisioned_runtime)
            .await?;
        self.confirm(
            UpsertAgentBindingRequest {
                session_id: session.clone(),
                client_type: client.into(),
                client_session_key: native_key.into(),
                launch_cwd: identity.launch_cwd,
                client_session_file: identity.client_session_file,
                metadata: json!({}),
            },
            &observation.instance_id,
            None,
            &observation.capabilities,
            observation.details,
        )
        .await?;
        Ok(session)
    }

    async fn observed_session(&self, client: &str, cwd: &str) -> Result<String> {
        let session = new_session_id().to_string();
        let workspace = crate::upsert_workspace(&self.pool, cwd).await?;
        self.events
            .ingest_pontia_event(PontiaEvent::new(
                &session,
                None,
                PontiaEventSource::RuntimeManager,
                client,
                PontiaEventType::SessionCreated,
                json!({"workspace":cwd}),
            ))
            .await?;
        pontia_storage_sqlite::repositories::sessions::SqliteSessionRepository::new(
            self.pool.clone(),
        )
        .update_session_workspace(&session, Some(cwd), Some(&workspace.workspace_id))
        .await?;
        Ok(session)
    }

    pub async fn ready(
        &self,
        session: &str,
        instance: &str,
        root: &Path,
        data: Value,
    ) -> Result<()> {
        crate::runtime::control_target::ControlTarget::resolve(&self.pool, session, Some(instance))
            .await?;
        let needs_ready =
            crate::SessionCommandService::new(self.events.clone(), root.to_path_buf())
                .observe_resumed_session(session, instance)
                .await?;
        let already: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE session_id=? AND event_type='session.ready' AND json_extract(payload,'$.runtime_instance_id')=?").bind(session).bind(instance).fetch_one(&self.pool).await?;
        if already == 0 || needs_ready {
            self.report(session, instance, EventType::SessionReady, data)
                .await?;
        }
        Ok(())
    }

    pub async fn exited(&self, session: &str, instance: &str, reason: &str) -> Result<()> {
        let state: String = sqlx::query_scalar("SELECT state FROM sessions WHERE session_id=?")
            .bind(session)
            .fetch_one(&self.pool)
            .await?;
        if state != "exited" {
            self.report(
                session,
                instance,
                EventType::SessionExited,
                json!({"reason":reason}),
            )
            .await?;
        }
        Ok(())
    }

    pub async fn observe_turn(
        &self,
        session: &str,
        instance: &str,
        turn: NativeTurnObservation,
    ) -> Result<()> {
        crate::runtime::control_target::ControlTarget::resolve(&self.pool, session, Some(instance))
            .await?;
        let native = &turn.native_turn_id;
        let existing: Option<(String,String)> = sqlx::query_as("SELECT t.turn_id,t.state FROM native_turn_bindings b JOIN turns t ON t.turn_id=b.turn_id WHERE b.session_id=? AND b.client_turn_id=?").bind(session).bind(native).fetch_optional(&self.pool).await?;
        if existing.as_ref().is_some_and(|(_, state)| {
            matches!(state.as_str(), "completed" | "failed" | "interrupted")
        }) {
            return Ok(());
        }
        if existing.is_none() {
            let dispatch = InboxCommandService::new(self.events.clone())
                .native_dispatch(session, native)
                .await?;
            let summary = turn
                .input_summary
                .as_deref()
                .or_else(|| dispatch.as_ref().map(|(_, input)| input.as_str()));
            self.report(session, instance, EventType::TurnStarted, json!({"native_turn_id":native,"input":{"summary":summary.map(|s| s.chars().take(200).collect::<String>())},"metadata":{"native_turn_id":native,"native_started_at":turn.started_at,"observation":turn.origin,"inbox_message_id":dispatch.map(|(id,_)|id)}})).await?;
        }
        InboxCommandService::new(self.events.clone())
            .link_native_turn(session, native, Some(instance))
            .await?;
        if let Some(kind) = turn.terminal? {
            if !matches!(
                kind,
                EventType::TurnCompleted | EventType::TurnFailed | EventType::TurnInterrupted
            ) {
                return Err(Error::Domain("Invalid native terminal fact".into()));
            }
            if let Some(text) = turn.output_summary {
                self.report(
                    session,
                    instance,
                    EventType::TurnOutput,
                    json!({"native_turn_id":native,"output":{"summary":text}}),
                )
                .await?;
            }
            self.report(session, instance, kind, json!({"native_turn_id":native,"native_completed_at":turn.completed_at,"observation":turn.origin,"failure":{"message":turn.failure}})).await?;
        }
        Ok(())
    }

    async fn report(
        &self,
        session: &str,
        instance: &str,
        kind: EventType,
        mut data: Value,
    ) -> Result<()> {
        data["runtime_instance_id"] = json!(instance);
        self.events
            .report_fact(ReportedFact {
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
}

#[cfg(test)]
mod tests;
