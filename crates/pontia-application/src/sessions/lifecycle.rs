use std::{collections::BTreeMap, path::Path};

use pontia_core::error::{Error, Result};
use pontia_runtime::RuntimeStartRequest;
use pontia_storage_sqlite::repositories::{
    turns::SqliteTurnRepository, workflows::SqliteWorkflowRepository,
};
use serde_json::json;

use super::SessionCommandService;
use crate::ControlCommandOutcome;
use crate::{
    PontiaEvent, PontiaEventSource, PontiaEventType, client_contract::ClientExitOutcome,
    get_workspace_record,
};
use pontia_core::domain::EventType;

impl SessionCommandService {
    pub async fn ensure_current_runtime(&self, session_id: &str, runtime_id: &str) -> Result<()> {
        self.ensure_control_target(session_id, Some(runtime_id))
            .await
    }

    pub async fn ensure_control_target(
        &self,
        session_id: &str,
        runtime_id: Option<&str>,
    ) -> Result<()> {
        let session = self
            .queries
            .get_session_control(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        let target =
            crate::runtime::ControlTarget::resolve(&self.pool, session_id, runtime_id).await?;
        self.clients
            .for_client(&session.client_type)?
            .ensure_exit_available(&target)
            .await
    }

    pub async fn terminate_session(&self, session_id: &str) -> Result<ControlCommandOutcome> {
        self.request_exit(session_id, None).await
    }

    pub async fn request_exit(
        &self,
        session_id: &str,
        expected_runtime: Option<&str>,
    ) -> Result<ControlCommandOutcome> {
        let query = &self.queries;
        let session = query
            .get_session_control(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        let target =
            crate::runtime::ControlTarget::resolve(&self.pool, session_id, expected_runtime)
                .await?;
        if !matches!(session.state.as_str(), "exited" | "error") {
            let outcome = self
                .clients
                .for_client(&session.client_type)?
                .exit(&target)
                .await
                .into_result()?;
            if let ClientExitOutcome::Confirmed { reason } = outcome {
                self.event_ingest
                    .report_client_fact(
                        session_id,
                        None,
                        EventType::SessionExited,
                        json!({"reason":reason}),
                    )
                    .await?;
            }
        }
        Ok(ControlCommandOutcome {
            data: json!({"session":query.get_session(session_id).await?}),
            duplicate: false,
        })
    }

    pub(crate) async fn resume_for_input(&self, session_id: &str) -> Result<()> {
        self.resume_session(session_id, &self.pontia_home).await?;
        let session = self
            .queries
            .get_session_control(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Session {session_id} not found")))?;
        let target = crate::runtime::ControlTarget::resolve(&self.pool, session_id, None).await?;
        self.clients
            .for_client(&session.client_type)?
            .await_initial_ready(&target)
            .await
    }

    pub async fn resume_session(
        &self,
        session_id: &str,
        pontia_home: &Path,
    ) -> Result<ControlCommandOutcome> {
        let _launch_guard = self.control.lock_identity().await;
        let query = &self.queries;
        let session = query
            .get_session_control(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        if session.state != "exited" {
            return Err(Error::StateConflict(format!(
                "session {session_id} in state {} cannot be resumed",
                session.state
            )));
        }
        let adapter = self.clients.for_client(&session.client_type)?;
        let target = crate::runtime::ControlTarget::resolve(&self.pool, session_id, None).await?;
        adapter.validate_resume(&target).await?;
        self.control.retire_connection_locked(session_id).await;
        let ingest = self.event_ingest.clone();
        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.to_string(),
                None,
                PontiaEventSource::ExternalApi,
                session.client_type.clone(),
                PontiaEventType::SessionResuming,
                json!({"runtime_id": target.runtime_id}),
            ))
            .await?;
        let runtime_workspace_name = if let Some(workspace_id) = session.workspace_id.as_deref() {
            get_workspace_record(&self.pool, workspace_id)
                .await?
                .and_then(|workspace| workspace.name)
        } else {
            None
        };
        let persisted_start_command = self.start_command(session_id).await?;
        let runtime = adapter
            .resume(
                &target,
                pontia_home,
                RuntimeStartRequest {
                    runtime_id: target.runtime_id.clone(),
                    session_id: session_id.to_string(),
                    client_type: session.client_type.clone(),
                    workspace: session.workspace.clone(),
                    workspace_name: runtime_workspace_name,
                    handle: session.handle.clone(),
                    role: session.role.clone(),
                    start_command: persisted_start_command,
                    environment: self.workflow_runtime_environment(session_id).await?,
                },
                1,
            )
            .await;
        let runtime = match runtime {
            Ok(runtime) => runtime,
            Err(error) => {
                ingest.ingest_pontia_event(PontiaEvent::new(session_id, None, PontiaEventSource::RuntimeManager, &session.client_type, PontiaEventType::SessionError,
                    json!({"runtime_id": target.runtime_id, "reason": "startup_failed", "failure": {"message": error.to_string()}}))).await?;
                return Err(error);
            }
        };
        let Some(runtime) = runtime else {
            return Ok(ControlCommandOutcome {
                data: json!({"session":query.get_session(session_id).await?}),
                duplicate: false,
            });
        };
        self.upsert_resumed_runtime_binding(session_id, &runtime)
            .await?;
        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.to_string(),
                None,
                PontiaEventSource::RuntimeManager,
                session.client_type.clone(),
                PontiaEventType::SessionStarted,
                json!({}),
            ))
            .await?;
        ingest
            .ingest_in_process_ready_event(&session.client_type, session_id, runtime.runtime_id())
            .await?;

        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::Domain("resumed session missing".to_string()))?;
        let data = json!({ "session": session });
        Ok(ControlCommandOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn restart_session(
        &self,
        session_id: &str,
        pontia_home: &Path,
    ) -> Result<ControlCommandOutcome> {
        let query = &self.queries;
        let session = query
            .get_session_control(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        if matches!(session.state.as_str(), "exited" | "error") {
            return Err(Error::StateConflict(format!(
                "terminal session {session_id} cannot be restarted"
            )));
        }
        if self
            .clients
            .for_client(&session.client_type)?
            .spec
            .adapter
            .lifecycle
            .restart_requires_exit
        {
            if SqliteTurnRepository::new(self.pool.clone())
                .active_turn(session_id)
                .await?
                .is_some()
            {
                return Err(Error::StateConflict(format!(
                    "session {session_id} has an active Turn and its runtime cannot be replaced"
                )));
            }
            let target =
                crate::runtime::ControlTarget::resolve(&self.pool, session_id, None).await?;
            self.clients
                .for_client(&session.client_type)?
                .validate_resume(&target)
                .await?;
            self.request_exit(session_id, target.runtime_id.as_deref())
                .await?;
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
            loop {
                if self
                    .queries
                    .get_session_control(session_id)
                    .await?
                    .is_some_and(|session| session.state == "exited")
                {
                    break;
                }
                if tokio::time::Instant::now() >= deadline {
                    return Err(Error::ControlUnknown(
                        "Agent client has not confirmed exit before restart".into(),
                    ));
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            return self.resume_session(session_id, pontia_home).await;
        }
        let adapter = self.clients.for_client(&session.client_type)?;
        if !adapter.supports_restart() {
            return Err(Error::CapabilityUnavailable(
                "A Session cannot restart a shared runtime".into(),
            ));
        }
        let target = crate::runtime::ControlTarget::resolve(&self.pool, session_id, None).await?;
        let prior_restart_count = 0;
        let mut runtime_replacement_tx = self.pool.begin().await?;
        SqliteTurnRepository::serialize_session_turn_writes_in_tx(
            &mut runtime_replacement_tx,
            session_id,
        )
        .await?;
        if SqliteTurnRepository::active_turn_in_tx(&mut runtime_replacement_tx, session_id)
            .await?
            .is_some()
        {
            return Err(Error::StateConflict(format!(
                "session {session_id} has an active Turn and its runtime cannot be replaced"
            )));
        }
        let runtime_workspace_name = if let Some(workspace_id) = session.workspace_id.as_deref() {
            get_workspace_record(&self.pool, workspace_id)
                .await?
                .and_then(|workspace| workspace.name)
        } else {
            None
        };
        let runtime = adapter
            .restart(
                &target,
                pontia_home,
                RuntimeStartRequest {
                    runtime_id: target.runtime_id.clone(),
                    session_id: session_id.to_string(),
                    client_type: session.client_type.clone(),
                    workspace: session.workspace.clone(),
                    workspace_name: runtime_workspace_name,
                    handle: session.handle.clone(),
                    role: session.role.clone(),
                    start_command: self.start_command(session_id).await?,
                    environment: self.workflow_runtime_environment(session_id).await?,
                },
                prior_restart_count + 1,
            )
            .await?;
        self.upsert_runtime_binding_in_tx(&mut runtime_replacement_tx, session_id, &runtime)
            .await?;
        runtime_replacement_tx.commit().await?;
        let ingest = self.event_ingest.clone();
        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.to_string(),
                None,
                PontiaEventSource::ExternalApi,
                session.client_type.clone(),
                PontiaEventType::SessionStarting,
                json!({"runtime_id": target.runtime_id}),
            ))
            .await?;
        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.to_string(),
                None,
                PontiaEventSource::RuntimeManager,
                session.client_type.clone(),
                PontiaEventType::SessionStarted,
                json!({}),
            ))
            .await?;
        ingest
            .ingest_in_process_ready_event(&session.client_type, session_id, runtime.runtime_id())
            .await?;

        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::Domain("restarted session missing".to_string()))?;
        let data = json!({ "session": session });
        Ok(ControlCommandOutcome {
            data,
            duplicate: false,
        })
    }

    async fn workflow_runtime_environment(
        &self,
        session_id: &str,
    ) -> Result<BTreeMap<String, String>> {
        let workflow_id = SqliteWorkflowRepository::new(self.pool.clone())
            .get_node_by_session(session_id)
            .await?
            .map(|node| node.workflow_id);
        Ok(workflow_id
            .map(|workflow_id| BTreeMap::from([("PONTIA_WORKFLOW_ID".to_string(), workflow_id)]))
            .unwrap_or_default())
    }
}
