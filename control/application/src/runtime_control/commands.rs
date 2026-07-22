use super::*;
use pontia_agent_clients::{ReadinessMode, TerminateBehavior, get_client_spec};
use pontia_storage_sqlite::repositories::turns::SqliteTurnRepository;

impl RuntimeControlService {
    pub async fn interrupt_current_turn(&self, session_id: &str) -> Result<ControlCommandOutcome> {
        let query = ExternalQueryService::new(self.pool.clone());
        if query.get_session(session_id).await?.is_none() {
            return Err(Error::NotFound(format!("session {session_id} not found")));
        }
        let active_turn = SqliteTurnRepository::new(self.pool.clone())
            .active_turn(session_id)
            .await?
            .ok_or_else(|| {
                Error::StateConflict(format!(
                    "session {session_id} has no active turn to interrupt"
                ))
            })?;
        self.interrupt_turn(session_id, &active_turn.turn_id).await
    }

    pub async fn interrupt_turn(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<ControlCommandOutcome> {
        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        let turn = query
            .get_turn(session_id, turn_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("turn {turn_id} not found")))?;

        if matches!(
            turn.state.as_str(),
            "completed" | "failed" | "interrupted" | "cancelled"
        ) {
            return Err(Error::StateConflict(format!(
                "turn {turn_id} is already terminal"
            )));
        }
        if SqliteTurnRepository::new(self.pool.clone())
            .active_turn(session_id)
            .await?
            .as_ref()
            .map(|turn| turn.turn_id.as_str())
            != Some(turn_id)
        {
            return Err(Error::StateConflict(format!(
                "turn {turn_id} is not the active turn for session {session_id}"
            )));
        }
        if !session.capabilities.interrupt {
            return Err(Error::CapabilityUnavailable(format!(
                "session {session_id} runtime does not support interrupt"
            )));
        }
        let tmux_binding = self.tmux_pane_binding(session_id).await?.ok_or_else(|| {
            Error::CapabilityUnavailable(format!(
                "session {session_id} runtime does not support interrupt: missing tmux pane binding"
            ))
        })?;
        let interrupt_behavior = get_client_spec(&session.client_type)
            .map(|spec| spec.adapter.interrupt)
            .ok_or_else(|| {
                Error::Domain(format!("unsupported client_type: {}", session.client_type))
            })?;
        self.runtime.interrupt_session(
            &tmux_binding.socket_path,
            &tmux_binding.pane_id,
            interrupt_behavior,
        )?;

        let ingest = EventIngestService::new(self.pool.clone());
        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.to_string(),
                Some(turn_id.to_string()),
                PontiaEventSource::ExternalApi,
                session.client_type.clone(),
                PontiaEventType::TurnInterruptRequested,
                json!({}),
            ))
            .await?;
        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.to_string(),
                Some(turn_id.to_string()),
                PontiaEventSource::RuntimeManager,
                session.client_type,
                PontiaEventType::TurnInterrupted,
                json!({}),
            ))
            .await?;

        let turn = query
            .get_turn(session_id, turn_id)
            .await?
            .ok_or_else(|| Error::Domain("interrupted turn missing".to_string()))?;
        let data = json!({ "turn": turn });
        Ok(ControlCommandOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn terminate_session(&self, session_id: &str) -> Result<ControlCommandOutcome> {
        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;

        if !matches!(session.state.as_str(), "exited" | "error") {
            let terminate_behavior = get_client_spec(&session.client_type)
                .map(|spec| spec.adapter.terminate)
                .ok_or_else(|| {
                    Error::Domain(format!("unsupported client_type: {}", session.client_type))
                })?;
            match terminate_behavior {
                TerminateBehavior::TmuxSendKeys(keys) => {
                    if let Some(tmux_binding) = self.tmux_pane_binding(session_id).await? {
                        self.runtime.terminate_tmux_pane(
                            &tmux_binding.socket_path,
                            &tmux_binding.pane_id,
                            keys,
                        )?;
                    }
                }
                TerminateBehavior::RuntimeManager => {
                    if let Some(runtime_target) = self.runtime_target(session_id).await? {
                        self.runtime.terminate_session(&runtime_target)?;
                    }
                }
            }
            EventIngestService::new(self.pool.clone())
                .ingest_pontia_event(PontiaEvent::new(
                    session_id.to_string(),
                    None,
                    PontiaEventSource::ExternalApi,
                    session.client_type.clone(),
                    PontiaEventType::SessionExited,
                    json!({ "reason": "terminate_requested" }),
                ))
                .await?;
        }

        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::Domain("terminated session missing".to_string()))?;
        let data = json!({ "session": session });
        Ok(ControlCommandOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn resume_session(&self, session_id: &str) -> Result<ControlCommandOutcome> {
        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        if session.state != "exited" {
            return Err(Error::StateConflict(format!(
                "session {session_id} in state {} cannot be resumed",
                session.state
            )));
        }

        let prior_restart_count = self.restart_count(session_id).await?.unwrap_or(0);
        let ingest = EventIngestService::new(self.pool.clone());
        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.to_string(),
                None,
                PontiaEventSource::ExternalApi,
                session.client_type.clone(),
                PontiaEventType::SessionResuming,
                json!({}),
            ))
            .await?;
        let tmux_binding = self.tmux_pane_binding(session_id).await?;
        let runtime_workspace_name = if let Some(workspace_id) = session.workspace_id.as_deref() {
            get_workspace_record(&self.pool, workspace_id)
                .await?
                .and_then(|workspace| workspace.name)
        } else {
            None
        };
        let runtime = self
            .runtime
            .start_session_with_restart_count_and_reuse_target(
                RuntimeStartRequest {
                    session_id: session_id.to_string(),
                    client_type: session.client_type.clone(),
                    workspace: session.workspace.clone(),
                    workspace_name: runtime_workspace_name,
                    handle: session.handle.clone(),
                    role: session.role.clone(),
                    start_command: self
                        .resume_start_command(session_id, &session.client_type)
                        .await?,
                },
                prior_restart_count + 1,
                tmux_binding
                    .as_ref()
                    .map(|binding| (binding.socket_path.as_str(), binding.pane_id.as_str())),
            )?;
        self.upsert_runtime_binding(session_id, &runtime).await?;
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
        if client_readiness_mode(&session.client_type)? == ReadinessMode::RuntimeManagerImmediate {
            ingest
                .ingest_pontia_event(PontiaEvent::new(
                    session_id.to_string(),
                    None,
                    PontiaEventSource::RuntimeManager,
                    session.client_type,
                    PontiaEventType::SessionReady,
                    json!({}),
                ))
                .await?;
        }

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

    pub async fn restart_session(&self, session_id: &str) -> Result<ControlCommandOutcome> {
        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        if matches!(session.state.as_str(), "exited" | "error") {
            return Err(Error::StateConflict(format!(
                "terminal session {session_id} cannot be restarted"
            )));
        }

        let prior_restart_count = self.restart_count(session_id).await?.unwrap_or(0);
        let terminate_behavior = get_client_spec(&session.client_type)
            .map(|spec| spec.adapter.terminate)
            .ok_or_else(|| {
                Error::Domain(format!("unsupported client_type: {}", session.client_type))
            })?;
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
        match terminate_behavior {
            TerminateBehavior::TmuxSendKeys(_) => {
                let tmux_binding = self.tmux_pane_binding(session_id).await?.ok_or_else(|| {
                    Error::CapabilityUnavailable(format!(
                        "session {session_id} runtime does not support restart: missing tmux pane binding"
                    ))
                })?;
                self.runtime
                    .kill_tmux_pane(&tmux_binding.socket_path, &tmux_binding.pane_id)?;
            }
            TerminateBehavior::RuntimeManager => {
                if let Some(runtime_target) = self.runtime_target(session_id).await? {
                    self.runtime.terminate_session(&runtime_target)?;
                }
            }
        }

        let runtime_workspace_name = if let Some(workspace_id) = session.workspace_id.as_deref() {
            get_workspace_record(&self.pool, workspace_id)
                .await?
                .and_then(|workspace| workspace.name)
        } else {
            None
        };
        let runtime = self.runtime.start_session_with_restart_count(
            RuntimeStartRequest {
                session_id: session_id.to_string(),
                client_type: session.client_type.clone(),
                workspace: session.workspace.clone(),
                workspace_name: runtime_workspace_name,
                handle: session.handle.clone(),
                role: session.role.clone(),
                start_command: self.start_command(session_id).await?,
            },
            prior_restart_count + 1,
        )?;
        self.upsert_runtime_binding_in_tx(&mut runtime_replacement_tx, session_id, &runtime)
            .await?;
        runtime_replacement_tx.commit().await?;
        let ingest = EventIngestService::new(self.pool.clone());
        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.to_string(),
                None,
                PontiaEventSource::ExternalApi,
                session.client_type.clone(),
                PontiaEventType::SessionStarting,
                json!({}),
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
        if client_readiness_mode(&session.client_type)? == ReadinessMode::RuntimeManagerImmediate {
            ingest
                .ingest_pontia_event(PontiaEvent::new(
                    session_id.to_string(),
                    None,
                    PontiaEventSource::RuntimeManager,
                    session.client_type,
                    PontiaEventType::SessionReady,
                    json!({}),
                ))
                .await?;
        }

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
}
