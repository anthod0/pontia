use super::*;
use crate::agent_clients::{ReadinessMode, get_client_spec};

impl RuntimeControlService {
    pub async fn interrupt_current_turn(
        &self,
        session_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<ControlCommandOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("interrupt_current:{session_id}"), key)
                .await?
        {
            return Ok(ControlCommandOutcome {
                data: response,
                duplicate: true,
            });
        }

        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        let turn_id = session.current_turn_id.clone().ok_or_else(|| {
            Error::StateConflict(format!(
                "session {session_id} has no active turn to interrupt"
            ))
        })?;
        let outcome = self.interrupt_turn(session_id, &turn_id, None).await?;
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(
                &format!("interrupt_current:{session_id}"),
                key,
                &outcome.data,
            )
            .await?;
        }
        Ok(outcome)
    }

    pub async fn interrupt_turn(
        &self,
        session_id: &str,
        turn_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<ControlCommandOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("interrupt_turn:{session_id}:{turn_id}"), key)
                .await?
        {
            return Ok(ControlCommandOutcome {
                data: response,
                duplicate: true,
            });
        }

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
        if session.current_turn_id.as_deref() != Some(turn_id) {
            return Err(Error::StateConflict(format!(
                "turn {turn_id} is not the active turn for session {session_id}"
            )));
        }
        if !session.capabilities.interrupt {
            return Err(Error::CapabilityUnavailable(format!(
                "session {session_id} runtime does not support interrupt"
            )));
        }
        let runtime_target = self.runtime_target(session_id).await?.ok_or_else(|| {
            Error::StateConflict(format!("session {session_id} has no runtime binding"))
        })?;
        let interrupt_behavior = get_client_spec(&session.client_type)
            .map(|spec| spec.interrupt)
            .ok_or_else(|| {
                Error::Domain(format!("unsupported client_type: {}", session.client_type))
            })?;
        self.runtime
            .interrupt_session(&runtime_target, interrupt_behavior)?;

        let ingest = EventIngestService::new(self.pool.clone());
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                Some(turn_id.to_string()),
                EventSource::ExternalApi,
                session.client_type.clone(),
                EventType::TurnInterruptRequested,
                json!({}),
            ))
            .await?;
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                Some(turn_id.to_string()),
                EventSource::RuntimeManager,
                session.client_type,
                EventType::TurnInterrupted,
                json!({}),
            ))
            .await?;

        let turn = query
            .get_turn(session_id, turn_id)
            .await?
            .ok_or_else(|| Error::Domain("interrupted turn missing".to_string()))?;
        let data = json!({ "turn": turn });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(
                &format!("interrupt_turn:{session_id}:{turn_id}"),
                key,
                &data,
            )
            .await?;
        }
        Ok(ControlCommandOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn terminate_session(
        &self,
        session_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<ControlCommandOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("terminate_session:{session_id}"), key)
                .await?
        {
            return Ok(ControlCommandOutcome {
                data: response,
                duplicate: true,
            });
        }

        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;

        if !matches!(session.state.as_str(), "exited" | "error") {
            if let Some(runtime_target) = self.runtime_target(session_id).await? {
                self.runtime.terminate_session(&runtime_target)?;
            }
            EventIngestService::new(self.pool.clone())
                .ingest_event(DomainEvent::new(
                    new_event_id().to_string(),
                    session_id.to_string(),
                    None,
                    EventSource::RuntimeManager,
                    session.client_type,
                    EventType::SessionExited,
                    json!({}),
                ))
                .await?;
        }

        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::Domain("terminated session missing".to_string()))?;
        let data = json!({ "session": session });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(&format!("terminate_session:{session_id}"), key, &data)
                .await?;
        }
        Ok(ControlCommandOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn resume_session(
        &self,
        session_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<ControlCommandOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("resume_session:{session_id}"), key)
                .await?
        {
            return Ok(ControlCommandOutcome {
                data: response,
                duplicate: true,
            });
        }

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
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                None,
                EventSource::ExternalApi,
                session.client_type.clone(),
                EventType::SessionResuming,
                json!({}),
            ))
            .await?;
        let runtime = self.runtime.start_session_with_restart_count(
            RuntimeStartRequest {
                session_id: session_id.to_string(),
                client_type: session.client_type.clone(),
                workspace: session.workspace.clone(),
                handle: session.handle.clone(),
                role: session.role.clone(),
                agent_kind: pontia_agent_kind(&session.metadata),
            },
            prior_restart_count + 1,
        )?;
        self.upsert_runtime_binding(session_id, &runtime).await?;
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                None,
                EventSource::RuntimeManager,
                session.client_type.clone(),
                EventType::SessionStarted,
                json!({}),
            ))
            .await?;
        if client_readiness_mode(&session.client_type)? == ReadinessMode::RuntimeManagerImmediate {
            ingest
                .ingest_event(DomainEvent::new(
                    new_event_id().to_string(),
                    session_id.to_string(),
                    None,
                    EventSource::RuntimeManager,
                    session.client_type,
                    EventType::SessionReady,
                    json!({}),
                ))
                .await?;
        }

        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::Domain("resumed session missing".to_string()))?;
        let data = json!({ "session": session });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(&format!("resume_session:{session_id}"), key, &data)
                .await?;
        }
        Ok(ControlCommandOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn restart_session(
        &self,
        session_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<ControlCommandOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("restart_session:{session_id}"), key)
                .await?
        {
            return Ok(ControlCommandOutcome {
                data: response,
                duplicate: true,
            });
        }

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
        if let Some(runtime_target) = self.runtime_target(session_id).await? {
            self.runtime.terminate_session(&runtime_target)?;
        }

        let ingest = EventIngestService::new(self.pool.clone());
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                None,
                EventSource::ExternalApi,
                session.client_type.clone(),
                EventType::SessionStarting,
                json!({}),
            ))
            .await?;
        let runtime = self.runtime.start_session_with_restart_count(
            RuntimeStartRequest {
                session_id: session_id.to_string(),
                client_type: session.client_type.clone(),
                workspace: session.workspace.clone(),
                handle: session.handle.clone(),
                role: session.role.clone(),
                agent_kind: pontia_agent_kind(&session.metadata),
            },
            prior_restart_count + 1,
        )?;
        self.upsert_runtime_binding(session_id, &runtime).await?;
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                None,
                EventSource::RuntimeManager,
                session.client_type.clone(),
                EventType::SessionStarted,
                json!({}),
            ))
            .await?;
        if client_readiness_mode(&session.client_type)? == ReadinessMode::RuntimeManagerImmediate {
            ingest
                .ingest_event(DomainEvent::new(
                    new_event_id().to_string(),
                    session_id.to_string(),
                    None,
                    EventSource::RuntimeManager,
                    session.client_type,
                    EventType::SessionReady,
                    json!({}),
                ))
                .await?;
        }

        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::Domain("restarted session missing".to_string()))?;
        let data = json!({ "session": session });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(&format!("restart_session:{session_id}"), key, &data)
                .await?;
        }
        Ok(ControlCommandOutcome {
            data,
            duplicate: false,
        })
    }
}
