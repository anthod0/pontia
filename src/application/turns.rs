use super::*;
use crate::{
    adapters::GenericTestAdapter,
    agent_clients::{DispatchMode, ReadinessMode, get_client_spec},
};

#[derive(Clone)]
pub struct TurnCommandService {
    pool: SqlitePool,
    runtime: GenericRuntimeManager,
}

impl TurnCommandService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            runtime: GenericRuntimeManager,
        }
    }

    pub(crate) async fn create_and_dispatch_turn(
        &self,
        session_id: &str,
        input: String,
        metadata: Value,
    ) -> Result<TurnView> {
        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;

        let client_spec = get_client_spec(&session.client_type).ok_or_else(|| {
            Error::Domain(format!("unsupported client_type: {}", session.client_type))
        })?;
        let dispatch_mode = client_spec.dispatch_mode;
        let readiness_mode = client_spec.readiness_mode;
        let can_accept_turn = matches!(session.state.as_str(), "idle" | "interrupted")
            || (session.state == "starting" && dispatch_mode == DispatchMode::TmuxPaste);
        if !can_accept_turn {
            return Err(Error::StateConflict(format!(
                "session {session_id} in state {} cannot accept a new turn",
                session.state
            )));
        }

        if let Some(active_turn_id) = &session.current_turn_id {
            return Err(Error::StateConflict(format!(
                "session {session_id} already has active turn {active_turn_id}"
            )));
        }

        if !session.capabilities.accept_task {
            return Err(Error::Domain(format!(
                "session {session_id} runtime cannot accept tasks"
            )));
        }

        let turn_id = new_turn_id().to_string();
        let agent_input = AgentInput {
            session_id: session_id.to_string(),
            turn_id: turn_id.clone(),
            input: input.clone(),
        };

        let ingest = EventIngestService::new(self.pool.clone());
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                Some(turn_id.clone()),
                EventSource::ExternalApi,
                session.client_type.clone(),
                EventType::TurnCreated,
                json!({
                    "input": { "summary": input },
                    "metadata": metadata,
                }),
            ))
            .await?;
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                Some(turn_id.clone()),
                EventSource::ExternalApi,
                session.client_type.clone(),
                EventType::TurnQueued,
                json!({}),
            ))
            .await?;

        if dispatch_mode == DispatchMode::GenericTestAdapter {
            let behavior = GenericTestAdapter::behavior();
            if behavior.write_current_turn_context
                && let Some((_runtime_ref, binding_metadata)) =
                    self.runtime_binding_metadata(session_id).await?
            {
                write_client_current_turn_context(
                    &binding_metadata,
                    &agent_input,
                    &session.client_type,
                )?;
            }
            self.runtime.submit_input(agent_input.clone())?;
            if behavior.auto_start_turn {
                ingest
                    .ingest_event(DomainEvent::new(
                        new_event_id().to_string(),
                        session_id.to_string(),
                        Some(turn_id.clone()),
                        EventSource::AgentAdapter,
                        session.client_type.clone(),
                        EventType::TurnStarted,
                        json!({}),
                    ))
                    .await?;
            }
        }

        if dispatch_mode == DispatchMode::TmuxPaste {
            match self.runtime_binding_metadata(session_id).await? {
                Some((runtime_ref, binding_metadata)) => {
                    match self
                        .wait_for_tui_readiness_if_needed(
                            &session.client_type,
                            readiness_mode,
                            session_id,
                            &binding_metadata,
                        )
                        .await
                        .and_then(|()| {
                            write_client_current_turn_context(
                                &binding_metadata,
                                &agent_input,
                                &session.client_type,
                            )
                        })
                        .and_then(|()| {
                            self.runtime.dispatch_tui_turn(
                                &runtime_ref,
                                &session.client_type,
                                &agent_input,
                            )
                        }) {
                        Ok(()) => {
                            ingest
                                .ingest_event(DomainEvent::new(
                                    new_event_id().to_string(),
                                    session_id.to_string(),
                                    Some(turn_id.clone()),
                                    EventSource::AgentAdapter,
                                    session.client_type.clone(),
                                    EventType::TurnStarted,
                                    json!({}),
                                ))
                                .await?;
                        }
                        Err(error) => {
                            ingest
                                .ingest_event(DomainEvent::new(
                                    new_event_id().to_string(),
                                    session_id.to_string(),
                                    Some(turn_id.clone()),
                                    EventSource::RuntimeManager,
                                    session.client_type.clone(),
                                    EventType::TurnFailed,
                                    json!({ "failure": { "message": error.to_string() } }),
                                ))
                                .await?;
                        }
                    }
                }
                None => {
                    ingest
                        .ingest_event(DomainEvent::new(
                            new_event_id().to_string(),
                            session_id.to_string(),
                            Some(turn_id.clone()),
                            EventSource::RuntimeManager,
                            session.client_type.clone(),
                            EventType::TurnFailed,
                            json!({ "failure": { "message": format!("{} runtime binding not found", session.client_type) } }),
                        ))
                        .await?;
                }
            }
        }

        let mut turn = query
            .get_turn(session_id, &turn_id)
            .await?
            .ok_or_else(|| Error::Domain("submitted turn missing".to_string()))?;
        query.enrich_turn_view(&mut turn).await?;
        Ok(turn)
    }

    async fn wait_for_tui_readiness_if_needed(
        &self,
        client_type: &str,
        readiness_mode: ReadinessMode,
        session_id: &str,
        metadata: &Value,
    ) -> Result<()> {
        if readiness_mode != ReadinessMode::AgentClientEvent {
            return Ok(());
        }
        let runtime_instance_id = metadata["runtime_instance_id"].as_str().ok_or_else(|| {
            Error::Domain(format!(
                "{client_type} runtime metadata missing runtime_instance_id"
            ))
        })?;
        RuntimeReadinessService::new(self.pool.clone())
            .wait_until_ready(session_id, client_type, runtime_instance_id)
            .await
    }

    async fn runtime_binding_metadata(&self, session_id: &str) -> Result<Option<(String, Value)>> {
        let row =
            sqlx::query("SELECT runtime_ref, metadata FROM runtime_bindings WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await?;
        row.map(|row| {
            let runtime_ref: String = row.try_get("runtime_ref")?;
            let metadata: String = row.try_get("metadata")?;
            let metadata = serde_json::from_str(&metadata)?;
            Ok((runtime_ref, metadata))
        })
        .transpose()
    }
}

pub(crate) fn write_client_current_turn_context(
    metadata: &Value,
    input: &AgentInput,
    client_type: &str,
) -> Result<()> {
    let current_turn_file = metadata["current_turn_file"]
        .as_str()
        .map(PathBuf::from)
        .or_else(|| {
            metadata["runtime_dir"]
                .as_str()
                .map(|runtime_dir| Path::new(runtime_dir).join("current-turn.json"))
        })
        .ok_or_else(|| {
            Error::Domain(format!(
                "{client_type} runtime metadata missing current_turn_file"
            ))
        })?;
    if let Some(parent) = current_turn_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let internal_event_url = metadata["internal_event_url"]
        .as_str()
        .unwrap_or("http://127.0.0.1:8080/internal/v1/events");
    let runtime_instance_id = metadata["runtime_instance_id"].as_str().ok_or_else(|| {
        Error::Domain(format!(
            "{client_type} runtime metadata missing runtime_instance_id"
        ))
    })?;
    let context = json!({
        "session_id": input.session_id,
        "turn_id": input.turn_id,
        "input": input.input,
        "client_type": client_type,
        "runtime_instance_id": runtime_instance_id,
        "internal_event_url": internal_event_url,
    });
    std::fs::write(current_turn_file, serde_json::to_vec_pretty(&context)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::EventIngestService,
        domain::{DomainEvent, EventSource, EventType},
        ids::{new_event_id, new_session_id},
        storage::sqlite::{connect_sqlite, run_migrations},
    };
    use std::{process::Command, time::Duration};

    struct TmuxSessionGuard {
        tmux_session: String,
    }

    impl Drop for TmuxSessionGuard {
        fn drop(&mut self) {
            let _ = Command::new("tmux")
                .args(["kill-session", "-t", &self.tmux_session])
                .status();
        }
    }

    async fn test_pool() -> SqlitePool {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("turn-readiness.db");
        let _kept_dir = dir.keep();
        let database_url = format!("sqlite://{}", db_path.display());
        let db = connect_sqlite(&database_url).await.expect("connect");
        run_migrations(&db).await.expect("migrate");
        db
    }

    fn tmux_session_name(session_id: &str) -> String {
        let sanitized: String = session_id
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect();
        format!("llmparty_{sanitized}")
    }

    async fn ingest_session_event(
        service: &EventIngestService,
        session_id: &str,
        event_type: EventType,
        source: EventSource,
        payload: Value,
    ) {
        service
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                None,
                source,
                "pi".to_string(),
                event_type,
                payload,
            ))
            .await
            .expect("ingest event");
    }

    #[tokio::test]
    async fn pi_tmux_turn_dispatch_waits_for_agent_client_ready() {
        let pool = test_pool().await;
        let session_id = new_session_id().to_string();
        let runtime_ref = tmux_session_name(&session_id);
        let _guard = TmuxSessionGuard {
            tmux_session: runtime_ref.clone(),
        };
        let runtime_dir = tempfile::tempdir().expect("runtime dir");
        let current_turn_file = runtime_dir.path().join("current-turn.json");
        let runtime_instance_id = "rtinst_wait_for_ready";

        let status = Command::new("tmux")
            .args(["new-session", "-d", "-s", &runtime_ref, "sleep", "30"])
            .status()
            .expect("spawn tmux");
        assert!(status.success(), "tmux session should start");

        let ingest = EventIngestService::new(pool.clone());
        ingest_session_event(
            &ingest,
            &session_id,
            EventType::SessionCreated,
            EventSource::ExternalApi,
            json!({"metadata": {}}),
        )
        .await;
        ingest_session_event(
            &ingest,
            &session_id,
            EventType::SessionStarting,
            EventSource::ExternalApi,
            json!({}),
        )
        .await;
        ingest_session_event(
            &ingest,
            &session_id,
            EventType::SessionStarted,
            EventSource::RuntimeManager,
            json!({}),
        )
        .await;

        sqlx::query(
            "INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_ref, metadata) VALUES (?, 'tmux', ?, ?)",
        )
        .bind(&session_id)
        .bind(&runtime_ref)
        .bind(json!({
            "runtime_instance_id": runtime_instance_id,
            "current_turn_file": current_turn_file.display().to_string(),
            "capabilities": {
                "accept_task": true,
                "report_turn_started": true,
                "report_turn_finished": true,
                "interrupt": true,
                "stream_output": true,
                "heartbeat": false,
                "artifact_sources": true
            }
        }).to_string())
        .execute(&pool)
        .await
        .expect("insert runtime binding");

        let service = TurnCommandService::new(pool.clone());
        let dispatch_session_id = session_id.clone();
        let dispatch = tokio::spawn(async move {
            service
                .create_and_dispatch_turn(
                    &dispatch_session_id,
                    "hello after ready".to_string(),
                    json!({}),
                )
                .await
        });

        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(
            !dispatch.is_finished(),
            "pi tmux dispatch must wait for session.ready before completing"
        );

        ingest_session_event(
            &ingest,
            &session_id,
            EventType::SessionReady,
            EventSource::AgentClient,
            json!({"runtime_instance_id": runtime_instance_id}),
        )
        .await;

        let turn = tokio::time::timeout(Duration::from_secs(2), dispatch)
            .await
            .expect("dispatch should finish after ready")
            .expect("dispatch task should not panic")
            .expect("dispatch should succeed");
        assert_eq!(turn.state, "running");
    }
}
