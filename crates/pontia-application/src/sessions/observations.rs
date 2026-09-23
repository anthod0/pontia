use crate::{
    EventIngestService, PontiaEvent, PontiaEventSource, PontiaEventType, UpsertAgentBindingRequest,
};
use pontia_core::{Error, Result, domain::EventType, ids::new_session_id};
use pontia_runtime::RuntimeStartResult;
use serde_json::{Value, json};
use sqlx::SqlitePool;

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

impl NativeSessionService {
    pub fn new(pool: SqlitePool, events: EventIngestService) -> Self {
        Self { pool, events }
    }

    pub async fn provision(&self, session: &str, runtime: &RuntimeStartResult) -> Result<()> {
        crate::runtime::NativeRuntimeBindings::new(self.pool.clone())
            .provision(session, runtime)
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
        crate::runtime::NativeRuntimeBindings::new(self.pool.clone())
            .confirm(binding, instance, expected_instance, capabilities, details)
            .await
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

    pub async fn ready(&self, session: &str, instance: &str, data: Value) -> Result<()> {
        crate::runtime::ControlTarget::resolve(&self.pool, session, Some(instance)).await?;
        let session_row =
            pontia_storage_sqlite::repositories::sessions::SqliteSessionRepository::new(
                self.pool.clone(),
            )
            .get_session(session)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session} not found")))?;
        let needs_ready = matches!(session_row.state.as_str(), "exited" | "starting");
        if session_row.state == "exited" {
            self.events
                .ingest_runtime_observation_event(PontiaEvent::new(
                    session,
                    None,
                    PontiaEventSource::RuntimeManager,
                    session_row.client_type,
                    PontiaEventType::SessionResuming,
                    json!({"runtime_instance_id":instance}),
                ))
                .await?;
        }
        let already: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE session_id=? AND event_type='session.ready' AND json_extract(payload,'$.runtime_instance_id')=?").bind(session).bind(instance).fetch_one(&self.pool).await?;
        if already == 0 || needs_ready {
            self.events
                .report_native_fact(session, instance, EventType::SessionReady, data)
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
            self.events
                .report_native_fact(
                    session,
                    instance,
                    EventType::SessionExited,
                    json!({"reason":reason}),
                )
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
