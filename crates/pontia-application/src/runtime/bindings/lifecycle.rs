use pontia_core::{error::Result, ids::new_session_id};
use serde_json::json;

use super::{
    RuntimeBindingUpsertRequest, metadata::agent_binding_metadata, request::non_empty,
    service::RuntimeBindingUpsertService,
};
use crate::{
    EventIngestService, PontiaEvent, PontiaEventSource, PontiaEventType, UpsertAgentBindingRequest,
    WorkspaceRecord,
};

impl RuntimeBindingUpsertService {
    pub(super) async fn record_resume_lifecycle_for_exited_session(
        &self,
        session_id: &str,
        request: &RuntimeBindingUpsertRequest,
    ) -> Result<()> {
        let state: Option<String> =
            sqlx::query_scalar("SELECT state FROM sessions WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await?;
        if state.as_deref() != Some("exited") {
            return Ok(());
        }

        let ingest = EventIngestService::new(self.pool.clone());
        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.to_string(),
                None,
                PontiaEventSource::RuntimeManager,
                request.client_type.clone(),
                PontiaEventType::SessionResuming,
                json!({}),
            ))
            .await?;
        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.to_string(),
                None,
                PontiaEventSource::RuntimeManager,
                request.client_type.clone(),
                PontiaEventType::SessionStarted,
                json!({}),
            ))
            .await?;
        Ok(())
    }

    pub(super) async fn create_bound_session(
        &self,
        request: &RuntimeBindingUpsertRequest,
        workspace: &WorkspaceRecord,
    ) -> Result<String> {
        let session_id = new_session_id().to_string();
        let ingest = EventIngestService::new(self.pool.clone());
        ingest
            .ingest_pontia_event_with_agent_binding(
                PontiaEvent::new(
                    session_id.clone(),
                    None,
                    PontiaEventSource::RuntimeManager,
                    request.client_type.clone(),
                    PontiaEventType::SessionCreated,
                    json!({
                        "workspace": workspace.canonical_path,
                        "metadata": {
                            "created_by": "runtime_binding_upsert",
                            "client_session_key": request.client_session_key,
                        }
                    }),
                ),
                UpsertAgentBindingRequest {
                    session_id: session_id.clone(),
                    client_type: request.client_type.clone(),
                    launch_cwd: workspace.canonical_path.clone(),
                    client_session_key: request.client_session_key.clone(),
                    client_session_file: non_empty(request.client_session_file.as_deref()),
                    metadata: agent_binding_metadata(request),
                },
            )
            .await?;
        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.clone(),
                None,
                PontiaEventSource::RuntimeManager,
                request.client_type.clone(),
                PontiaEventType::SessionStarting,
                json!({}),
            ))
            .await?;
        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.clone(),
                None,
                PontiaEventSource::RuntimeManager,
                request.client_type.clone(),
                PontiaEventType::SessionStarted,
                json!({}),
            ))
            .await?;
        Ok(session_id)
    }
}
