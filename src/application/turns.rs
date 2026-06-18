use super::*;
use crate::{
    agent_clients,
    agent_clients::{DispatchMode, ReadinessMode, TurnContextBehavior, get_client_spec},
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
    ) -> Result<Option<TurnView>> {
        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;

        let client_spec = get_client_spec(&session.client_type).ok_or_else(|| {
            Error::Domain(format!("unsupported client_type: {}", session.client_type))
        })?;
        let dispatch_mode = client_spec.adapter.dispatch;
        let readiness_mode = client_spec.adapter.readiness;
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
            return Err(Error::CapabilityUnavailable(format!(
                "session {session_id} runtime cannot accept tasks"
            )));
        }
        let tmux_binding = if dispatch_mode == DispatchMode::TmuxPaste {
            Some(self.required_tmux_pane_binding(session_id).await?)
        } else {
            None
        };

        let plugin_owns_turn = client_spec.owns_interactive_tmux_turn(&metadata);

        if plugin_owns_turn {
            let binding_metadata = self
                .runtime_binding_metadata(session_id)
                .await?
                .ok_or_else(|| {
                    Error::Domain(format!("{} runtime binding not found", session.client_type))
                })?;
            let tmux_binding = tmux_binding
                .as_ref()
                .expect("tmux binding was validated before client dispatch");
            let agent_input = AgentInput {
                session_id: session_id.to_string(),
                turn_id: new_turn_id().to_string(),
                input,
            };
            self.wait_for_tui_readiness_if_needed(
                &session.client_type,
                readiness_mode,
                session_id,
                &binding_metadata,
            )
            .await?;
            if client_spec.adapter.turn_context == TurnContextBehavior::CurrentTurnFile {
                write_client_current_turn_context(
                    &binding_metadata,
                    &agent_input,
                    &session.client_type,
                    Some(&metadata),
                )?;
            }
            self.runtime.dispatch_tui_turn(
                &tmux_binding.socket_path,
                &tmux_binding.pane_id,
                &session.client_type,
                &agent_input,
            )?;
            return Ok(None);
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

        if dispatch_mode == DispatchMode::InProcessRecorded {
            let behavior =
                agent_clients::in_process_recorded_dispatch_behavior(&session.client_type)
                    .ok_or_else(|| {
                        Error::Domain(format!(
                            "{} does not support in-process recorded dispatch",
                            session.client_type
                        ))
                    })?;
            if behavior.write_current_turn_context
                && let Some(binding_metadata) = self.runtime_binding_metadata(session_id).await?
            {
                write_client_current_turn_context(
                    &binding_metadata,
                    &agent_input,
                    &session.client_type,
                    Some(&metadata),
                )?;
            }
            self.runtime
                .submit_input(&session.client_type, agent_input.clone())?;
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
                Some(binding_metadata) => {
                    let tmux_binding = tmux_binding
                        .as_ref()
                        .expect("tmux binding was validated before turn creation");
                    match self
                        .wait_for_tui_readiness_if_needed(
                            &session.client_type,
                            readiness_mode,
                            session_id,
                            &binding_metadata,
                        )
                        .await
                        .and_then(|()| {
                            if client_spec.adapter.turn_context
                                == TurnContextBehavior::CurrentTurnFile
                            {
                                write_client_current_turn_context(
                                    &binding_metadata,
                                    &agent_input,
                                    &session.client_type,
                                    Some(&metadata),
                                )?;
                            }
                            Ok(())
                        })
                        .and_then(|()| {
                            self.runtime.dispatch_tui_turn(
                                &tmux_binding.socket_path,
                                &tmux_binding.pane_id,
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
        Ok(Some(turn))
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

    async fn runtime_binding_metadata(&self, session_id: &str) -> Result<Option<Value>> {
        let row = sqlx::query("SELECT metadata FROM runtime_bindings WHERE session_id = ?")
            .bind(session_id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| {
            let metadata: String = row.try_get("metadata")?;
            serde_json::from_str(&metadata).map_err(Into::into)
        })
        .transpose()
    }

    async fn required_tmux_pane_binding(&self, session_id: &str) -> Result<TmuxPaneBinding> {
        let row = sqlx::query(
            "SELECT tmux_socket_path, tmux_pane_id FROM runtime_bindings WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::Domain(format!("session {session_id} has no runtime binding")))?;
        let socket_path: Option<String> = row.try_get("tmux_socket_path")?;
        let pane_id: Option<String> = row.try_get("tmux_pane_id")?;
        match (socket_path, pane_id) {
            (Some(socket_path), Some(pane_id))
                if !socket_path.trim().is_empty() && !pane_id.trim().is_empty() =>
            {
                Ok(TmuxPaneBinding {
                    socket_path,
                    pane_id,
                })
            }
            _ => Err(Error::CapabilityUnavailable(format!(
                "session {session_id} runtime cannot accept tasks: missing tmux pane binding"
            ))),
        }
    }
}

struct TmuxPaneBinding {
    socket_path: String,
    pane_id: String,
}

pub(crate) fn write_client_current_turn_context(
    metadata: &Value,
    input: &AgentInput,
    client_type: &str,
    turn_metadata: Option<&Value>,
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
        .map(ToString::to_string)
        .or_else(crate::runtime::configured_internal_event_url)
        .unwrap_or_else(|| "http://127.0.0.1:8080/internal/v1/events".to_string());
    let runtime_instance_id = metadata["runtime_instance_id"].as_str().ok_or_else(|| {
        Error::Domain(format!(
            "{client_type} runtime metadata missing runtime_instance_id"
        ))
    })?;
    let mut context = json!({
        "session_id": input.session_id,
        "input": input.input,
        "client_type": client_type,
        "runtime_instance_id": runtime_instance_id,
        "internal_event_url": internal_event_url,
    });
    let include_turn_id = get_client_spec(client_type)
        .map(|spec| spec.current_turn_context_includes_turn_id())
        .unwrap_or(true);
    if include_turn_id {
        context["turn_id"] = json!(input.turn_id);
    }
    if let Some(inbox_message_id) = turn_metadata
        .and_then(|metadata| metadata.get("inbox_message_id"))
        .and_then(Value::as_str)
    {
        context["inbox_message_id"] = json!(inbox_message_id);
    }
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
        format!("pontia_{sanitized}")
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

    #[test]
    fn pi_current_turn_context_omits_backend_turn_id() {
        let runtime_dir = tempfile::tempdir().expect("runtime dir");
        let current_turn_file = runtime_dir.path().join("current-turn.json");
        let input = AgentInput {
            session_id: "sess_plugin_owned".to_string(),
            turn_id: "turn_backend_should_not_escape".to_string(),
            input: "hello".to_string(),
        };

        write_client_current_turn_context(
            &json!({
                "current_turn_file": current_turn_file.display().to_string(),
                "runtime_instance_id": "rtinst_plugin_owned",
                "internal_event_url": "http://127.0.0.1:8080/internal/v1/events",
            }),
            &input,
            "pi",
            None,
        )
        .expect("write context");

        let context: Value =
            serde_json::from_slice(&std::fs::read(&current_turn_file).expect("read context"))
                .expect("json context");
        assert_eq!(context["session_id"], "sess_plugin_owned");
        assert_eq!(context["input"], "hello");
        assert!(
            context.get("turn_id").is_none(),
            "pi plugin must generate the authoritative turn id"
        );
    }

    #[tokio::test]
    async fn pi_tmux_turn_dispatch_requires_bound_tmux_pane_before_creating_turn() {
        let pool = test_pool().await;
        let session_id = new_session_id().to_string();
        let runtime_instance_id = "rtinst_no_pane";

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
            EventType::SessionStarted,
            EventSource::RuntimeManager,
            json!({}),
        )
        .await;
        ingest_session_event(
            &ingest,
            &session_id,
            EventType::SessionReady,
            EventSource::AgentClient,
            json!({"runtime_instance_id": runtime_instance_id}),
        )
        .await;

        sqlx::query(
            "INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, metadata) VALUES (?, 'pi_tui', ?, ?)",
        )
        .bind(&session_id)
        .bind(runtime_instance_id)
        .bind(json!({
            "runtime_instance_id": runtime_instance_id,
            "current_turn_file": "/tmp/unused-current-turn.json",
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

        let error = TurnCommandService::new(pool.clone())
            .create_and_dispatch_turn(&session_id, "cannot web write".to_string(), json!({}))
            .await
            .expect_err("missing pane binding should reject dispatch");
        assert!(
            error.to_string().contains("runtime cannot accept tasks"),
            "unexpected error: {error}"
        );
        let turn_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns WHERE session_id = ?")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .expect("turn count");
        assert_eq!(turn_count, 0);
    }

    #[tokio::test]
    async fn pi_tmux_turn_dispatch_waits_for_agent_client_ready() {
        let pool = test_pool().await;
        let session_id = new_session_id().to_string();
        let tmux_session_name = tmux_session_name(&session_id);
        let _guard = TmuxSessionGuard {
            tmux_session: tmux_session_name.clone(),
        };
        let runtime_dir = tempfile::tempdir().expect("runtime dir");
        let current_turn_file = runtime_dir.path().join("current-turn.json");
        let runtime_instance_id = "rtinst_wait_for_ready";

        let status = Command::new("tmux")
            .args(["new-session", "-d", "-s", &tmux_session_name, "sleep", "30"])
            .status()
            .expect("spawn tmux");
        assert!(status.success(), "tmux session should start");
        let socket_path = Command::new("tmux")
            .args([
                "display-message",
                "-p",
                "-t",
                &tmux_session_name,
                "#{socket_path}",
            ])
            .output()
            .expect("query socket path");
        assert!(
            socket_path.status.success(),
            "socket path query should succeed"
        );
        let socket_path = String::from_utf8(socket_path.stdout)
            .expect("socket path utf8")
            .trim()
            .to_string();
        let pane_id = Command::new("tmux")
            .args([
                "display-message",
                "-p",
                "-t",
                &tmux_session_name,
                "#{pane_id}",
            ])
            .output()
            .expect("query pane id");
        assert!(pane_id.status.success(), "pane id query should succeed");
        let pane_id = String::from_utf8(pane_id.stdout)
            .expect("pane id utf8")
            .trim()
            .to_string();

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
            "INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, tmux_socket_path, tmux_pane_id, metadata) VALUES (?, 'tmux', ?, ?, ?, ?)",
        )
        .bind(&session_id)
        .bind(runtime_instance_id)
        .bind(&socket_path)
        .bind(&pane_id)
        .bind(json!({
            "runtime_instance_id": runtime_instance_id,
            "tmux": { "session_name": tmux_session_name },
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

        tokio::time::timeout(Duration::from_secs(2), dispatch)
            .await
            .expect("dispatch should finish after ready")
            .expect("dispatch task should not panic")
            .expect("dispatch should succeed");
        let turn_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns WHERE session_id = ?")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .expect("turn count");
        assert_eq!(
            turn_count, 0,
            "tmux paste dispatch must not create authoritative turn facts before pi hook reports agent_start"
        );
    }
}
