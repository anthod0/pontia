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
    pub runtime_id: String,
}

impl NativeSessionService {
    pub fn new(pool: SqlitePool, events: EventIngestService) -> Self {
        Self { pool, events }
    }

    pub async fn record_fork(
        &self,
        child: &str,
        parent: &str,
        turn: Option<&str>,
        native_boundary: Option<&str>,
        metadata: Value,
    ) -> Result<()> {
        if child == parent {
            return Err(Error::StateConflict(
                "A fork must have an independent Session identity".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO session_lineage (child_session_id,parent_session_id,relation_type,forked_from_turn_id,forked_from_client_node_id,parent_client_session_key,child_client_session_key,metadata)
             SELECT ?,?,'fork',?,?,p.client_session_key,c.client_session_key,? FROM agent_bindings p JOIN agent_bindings c ON c.session_id=? WHERE p.session_id=?
             ON CONFLICT(child_session_id) DO UPDATE SET
             forked_from_turn_id=COALESCE(excluded.forked_from_turn_id,session_lineage.forked_from_turn_id),
             forked_from_client_node_id=COALESCE(excluded.forked_from_client_node_id,session_lineage.forked_from_client_node_id),
             metadata=CASE WHEN excluded.forked_from_client_node_id IS NOT NULL THEN excluded.metadata ELSE session_lineage.metadata END",
        ).bind(child).bind(parent).bind(turn).bind(native_boundary)
            .bind(serde_json::to_string(&metadata)?).bind(child).bind(parent)
            .execute(&self.pool).await?;
        Ok(())
    }

    /// Adopts a native conversation served by a shared backend, without creating a runtime.
    pub async fn resolve_subscription_session(
        &self,
        client: &str,
        native_key: &str,
        identity: NativeSessionIdentity,
        metadata: Value,
    ) -> Result<String> {
        let control = self.events.history_control();
        let _identity = match &control {
            Some(control) => Some(control.lock_identity().await),
            None => None,
        };
        if let Some(binding) = crate::AgentBindingService::new(self.pool.clone())
            .binding_for_client_session(client, native_key)
            .await?
        {
            return Ok(binding.session_id);
        }
        crate::agent_clients::AgentClientService::new(self.pool.clone(), self.events.clients())
            .for_creation(client, &metadata)
            .await?;
        let workspace = crate::upsert_workspace(&self.pool, &identity.launch_cwd).await?;
        let session = new_session_id().to_string();
        self.events
            .ingest_pontia_event_with_agent_binding(
                PontiaEvent::new(
                    &session,
                    None,
                    PontiaEventSource::RuntimeManager,
                    client,
                    PontiaEventType::SessionCreated,
                    json!({"workspace":workspace.canonical_path,"metadata":metadata}),
                ),
                UpsertAgentBindingRequest {
                    session_id: session.clone(),
                    client_type: client.into(),
                    client_session_key: native_key.into(),
                    launch_cwd: workspace.canonical_path.clone(),
                    client_session_file: identity.client_session_file,
                    metadata: json!({}),
                },
            )
            .await?;
        pontia_storage_sqlite::repositories::sessions::SqliteSessionRepository::new(
            self.pool.clone(),
        )
        .update_session_workspace(
            &session,
            Some(&workspace.canonical_path),
            Some(&workspace.workspace_id),
        )
        .await?;
        Ok(session)
    }

    pub async fn provision(&self, session: &str, runtime: &RuntimeStartResult) -> Result<()> {
        crate::runtime::NativeRuntimeBindings::new(self.pool.clone())
            .provision(session, runtime)
            .await
    }

    pub async fn confirm(
        &self,
        binding: UpsertAgentBindingRequest,
        runtime_id: &str,
    ) -> Result<()> {
        crate::runtime::NativeRuntimeBindings::new(self.pool.clone())
            .confirm(binding, runtime_id, &self.events.clients())
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
            &observation.runtime_id,
        )
        .await?;
        Ok(session)
    }

    async fn observed_session(&self, client: &str, cwd: &str) -> Result<String> {
        crate::agent_clients::AgentClientService::new(self.pool.clone(), self.events.clients())
            .for_creation(client, &json!({}))
            .await?;
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
        crate::runtime::ControlTarget::resolve(
            &self.pool,
            &self.events.clients(),
            session,
            Some(instance),
        )
        .await?;
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
                    json!({"runtime_id":instance}),
                ))
                .await?;
        }
        let already: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE session_id=? AND event_type='session.ready' AND json_extract(payload,'$.runtime_id')=?").bind(session).bind(instance).fetch_one(&self.pool).await?;
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
