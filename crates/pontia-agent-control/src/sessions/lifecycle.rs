use std::path::Path;

use pontia_core::error::{Error, Result};
use pontia_runtime::RuntimeStartRequest;
use pontia_storage_sqlite::repositories::turns::SqliteTurnRepository;
use serde_json::json;

use super::SessionCommandService;
use crate::SessionView;
use crate::{
    PontiaEvent, PontiaEventSource, PontiaEventType,
    client_contract::{ClientExitMode, ClientExitOutcome},
    get_workspace_record,
};
use pontia_core::domain::EventType;

impl SessionCommandService {
    pub async fn await_ready(&self, session_id: &str) -> Result<()> {
        let session = self
            .queries
            .get_session_control(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Session {session_id} not found")))?;
        let target = crate::runtime::ControlTarget::resolve(
            &self.pool,
            self.clients.registry(),
            session_id,
            None,
        )
        .await?;
        self.clients
            .for_client(&session.client_type)?
            .await_initial_ready(&target)
            .await
    }

    /// Serialize with input delivery and reject an exit if execution has resumed.
    pub async fn exit_if_idle(&self, session_id: &str) -> Result<bool> {
        let actor = self.inbox.scheduler.session(session_id);
        let _guard = actor.lock().await;
        if !self.request_idle_exit(session_id).await? {
            return Ok(false);
        }
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            if self
                .queries
                .session_snapshot(session_id)
                .await?
                .is_some_and(|session| session.state == "exited")
            {
                return Ok(true);
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(Error::ControlUnknown(
                    "Session exit has not been confirmed".into(),
                ));
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    /// Arm automatic exit for this Session, including after a daemon restart.
    /// Submit or prepare the intended input before arming: an already idle Session
    /// with no unsettled input is eligible immediately. The policy remains armed
    /// if the Session is later resumed. Interrupted, waiting, and error states do
    /// not count as done; the Session must return to idle.
    pub async fn exit_when_done(&self, session_id: &str) -> Result<()> {
        let changed = sqlx::query("UPDATE sessions SET exit_when_done=1 WHERE session_id=?")
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        if changed.rows_affected() == 0 {
            return Err(Error::NotFound(format!("Session {session_id} not found")));
        }
        self.inbox.notify_available(session_id);
        Ok(())
    }

    pub(crate) async fn auto_exit_if_done(&self, session_id: &str) -> Result<()> {
        let actor = self.inbox.scheduler.session(session_id);
        let _guard = actor.lock().await;
        if actor.phase() != crate::inbox::SessionPhase::Idle {
            return Ok(());
        }
        let armed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE session_id=? AND exit_when_done=1 AND state='idle')",
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;
        if armed {
            self.request_idle_exit(session_id).await?;
        }
        Ok(())
    }

    async fn request_idle_exit(&self, session_id: &str) -> Result<bool> {
        let snapshot = self
            .queries
            .session_snapshot(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Session {session_id} not found")))?;
        if snapshot.state == "exited" {
            return Ok(true);
        }
        if snapshot.state != "idle"
            || snapshot.has_unsettled_input
            || snapshot
                .turns
                .iter()
                .any(|turn| matches!(turn.state.as_str(), "queued" | "running"))
        {
            return Ok(false);
        }
        let target = crate::runtime::ControlTarget::resolve(
            &self.pool,
            self.clients.registry(),
            session_id,
            None,
        )
        .await?;
        self.exit_target(&snapshot.client_type, &target, ClientExitMode::IfIdle)
            .await
    }

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
        let target = crate::runtime::ControlTarget::resolve(
            &self.pool,
            self.clients.registry(),
            session_id,
            runtime_id,
        )
        .await?;
        self.clients
            .for_client(&session.client_type)?
            .ensure_exit_available(&target)
            .await
    }

    pub async fn terminate_session(&self, session_id: &str) -> Result<SessionView> {
        self.request_exit(session_id, None).await
    }

    pub async fn request_exit(
        &self,
        session_id: &str,
        expected_runtime: Option<&str>,
    ) -> Result<SessionView> {
        let query = &self.queries;
        let session = query
            .get_session_control(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        if session.state == "error" {
            let current = query
                .session_snapshot(session_id)
                .await?
                .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
            let runtimes = pontia_storage_sqlite::repositories::session_runtimes::SqliteSessionRuntimeRepository::new(self.pool.clone()).list(session_id).await?;
            if current.state == "error" && runtimes.iter().all(|runtime| runtime.state == "exited")
            {
                let target = crate::runtime::ControlTarget::resolve(
                    &self.pool,
                    self.clients.registry(),
                    session_id,
                    expected_runtime,
                )
                .await?;
                for runtime in runtimes
                    .iter()
                    .filter(|runtime| Some(&runtime.runtime_id) == target.runtime_id.as_ref())
                {
                    crate::runtime::terminate_runtime(
                        runtime.tmux_socket_path.as_deref(),
                        runtime.tmux_pane_id.as_deref(),
                        runtime.process_fingerprint.as_deref(),
                    )?;
                }
                self.event_ingest
                    .ingest_pontia_event(PontiaEvent::new(
                        session_id,
                        None,
                        PontiaEventSource::ExternalApi,
                        &session.client_type,
                        PontiaEventType::SessionExited,
                        json!({"reason":"management_exit", "runtime_id":target.runtime_id}),
                    ))
                    .await?;
                return self.command_session(session_id).await;
            }
        }
        let target = crate::runtime::ControlTarget::resolve(
            &self.pool,
            self.clients.registry(),
            session_id,
            expected_runtime,
        )
        .await?;
        if session.state != "exited" {
            self.exit_target(&session.client_type, &target, ClientExitMode::Unconditional)
                .await?;
        }
        self.command_session(session_id).await
    }

    async fn exit_target(
        &self,
        client_type: &str,
        target: &crate::runtime::ControlTarget,
        mode: ClientExitMode,
    ) -> Result<bool> {
        match self
            .clients
            .for_client(client_type)?
            .exit(target, mode)
            .await
            .into_result()?
        {
            ClientExitOutcome::Busy => return Ok(false),
            ClientExitOutcome::Requested => {}
            ClientExitOutcome::Confirmed { reason } => {
                self.event_ingest
                    .report_client_fact(
                        &target.session_id,
                        target.runtime_id.as_deref(),
                        EventType::SessionExited,
                        json!({"reason":reason}),
                    )
                    .await?;
            }
        }
        Ok(true)
    }

    pub async fn resume_for_input(&self, session_id: &str) -> Result<()> {
        self.resume_session(session_id, &self.pontia_home).await?;
        let session = self
            .queries
            .get_session_control(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Session {session_id} not found")))?;
        let target = crate::runtime::ControlTarget::resolve(
            &self.pool,
            self.clients.registry(),
            session_id,
            None,
        )
        .await?;
        self.clients
            .for_client(&session.client_type)?
            .await_initial_ready(&target)
            .await
    }

    async fn require_launch_directory(
        &self,
        session: &crate::queries::SessionControlState,
    ) -> Result<()> {
        if let Some(workspace_id) = session.workspace_id.as_deref() {
            crate::workspaces::require_active_workspace_id(&self.pool, workspace_id).await?;
        } else if self
            .queries
            .application_launch_directory(&session.session_id)
            .await?
            .is_none()
        {
            return Err(Error::StateConflict(
                "An active registered Workspace is required".into(),
            ));
        }
        Ok(())
    }

    pub async fn resume_session(
        &self,
        session_id: &str,
        pontia_home: &Path,
    ) -> Result<SessionView> {
        let _launch = pontia_runtime::launch_lock::launch(pontia_home)?;
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
        self.require_launch_directory(&session).await?;
        let adapter = self.clients.for_client(&session.client_type)?;
        let target = crate::runtime::ControlTarget::resolve(
            &self.pool,
            self.clients.registry(),
            session_id,
            None,
        )
        .await?;
        adapter.validate_resume(&target).await?;
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
        let persisted_start_command = self
            .start_command(session_id, target.control_runtime_role)
            .await?;
        let runtime = adapter
            .resume(
                &target,
                pontia_home,
                RuntimeStartRequest {
                    reasoning_effort: None,
                    launch_mode: self.queries.session_launch_mode(session_id).await?,
                    mode: self.queries.session_mode(session_id).await?,
                    model: None,
                    runtime_id: target.runtime_id.clone(),
                    session_id: session_id.to_string(),
                    client_type: session.client_type.clone(),
                    workspace: session.workspace.clone(),
                    workspace_name: runtime_workspace_name,
                    handle: session.handle.clone(),
                    role: session.role.clone(),
                    start_command: persisted_start_command,
                    environment: self.queries.runtime_environment(session_id).await?,
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
            return self.command_session(session_id).await;
        };
        self.upsert_resumed_runtime_binding(session_id, &runtime, target.control_runtime_role)
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

        self.command_session(session_id).await
    }

    pub async fn restart_session(
        &self,
        session_id: &str,
        pontia_home: &Path,
    ) -> Result<SessionView> {
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
        self.require_launch_directory(&session).await?;
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
            let target = crate::runtime::ControlTarget::resolve(
                &self.pool,
                self.clients.registry(),
                session_id,
                None,
            )
            .await?;
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
            return Err(Error::Unsupported(
                "A Session cannot restart a shared runtime".into(),
            ));
        }
        let target = crate::runtime::ControlTarget::resolve(
            &self.pool,
            self.clients.registry(),
            session_id,
            None,
        )
        .await?;
        let prior_restart_count = 0;
        let environment = self.queries.runtime_environment(session_id).await?;
        let mode = self.queries.session_mode(session_id).await?;
        let launch_mode = self.queries.session_launch_mode(session_id).await?;
        let _launch = pontia_runtime::launch_lock::launch(pontia_home)?;
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
                    reasoning_effort: None,
                    launch_mode,
                    mode,
                    model: None,
                    runtime_id: target.runtime_id.clone(),
                    session_id: session_id.to_string(),
                    client_type: session.client_type.clone(),
                    workspace: session.workspace.clone(),
                    workspace_name: runtime_workspace_name,
                    handle: session.handle.clone(),
                    role: session.role.clone(),
                    start_command: self
                        .start_command(session_id, target.control_runtime_role)
                        .await?,
                    environment,
                },
                prior_restart_count + 1,
            )
            .await?;
        self.upsert_runtime_binding_in_tx(
            &mut runtime_replacement_tx,
            session_id,
            &runtime,
            target.control_runtime_role,
        )
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

        self.command_session(session_id).await
    }
}
