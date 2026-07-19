use serde_json::{Value, json};
use sqlx::SqlitePool;

use pontia_agent_clients as agent_clients;
use pontia_agent_clients::{DispatchMode, ReadinessMode, TurnContextBehavior, get_client_spec};
use pontia_core::{
    domain::{EventSource, EventType, ReportedEvent},
    error::{Error, Result},
    ids::{new_dispatch_id, new_event_id, new_turn_id},
};
use pontia_runtime::{AgentInput, GenericRuntimeManager};
use pontia_storage_sqlite::repositories::runtime_bindings::SqliteRuntimeBindingRepository;

use super::{context::store_client_current_turn_context, tmux::TmuxPaneBinding};
use crate::{EventIngestService, ExternalQueryService, RuntimeReadinessService, TurnView};

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

    pub async fn create_and_dispatch_turn(
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
                dispatch_id: new_dispatch_id().to_string(),
                input,
            };
            self.wait_for_tui_readiness_if_needed(
                &session.client_type,
                readiness_mode,
                session_id,
                &binding_metadata,
            )
            .await?;
            let binding_metadata = self
                .runtime_binding_metadata(session_id)
                .await?
                .ok_or_else(|| {
                    Error::Domain(format!("{} runtime binding not found", session.client_type))
                })?;
            match client_spec.adapter.turn_context {
                TurnContextBehavior::InternalApiClaim => {
                    store_client_current_turn_context(
                        self.pool.clone(),
                        session_id,
                        &binding_metadata,
                        &agent_input,
                        &session.client_type,
                        Some(&metadata),
                    )
                    .await?;
                }
                TurnContextBehavior::Disabled => {}
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
            dispatch_id: new_dispatch_id().to_string(),
            input: input.clone(),
        };

        let ingest = EventIngestService::new(self.pool.clone());
        ingest
            .ingest_event(ReportedEvent::new(
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
            .ingest_event(ReportedEvent::new(
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
            self.runtime
                .submit_input(&session.client_type, agent_input.clone())?;
            if behavior.auto_start_turn {
                ingest
                    .ingest_event(ReportedEvent::new(
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
                    if client_spec.adapter.turn_context == TurnContextBehavior::InternalApiClaim {
                        store_client_current_turn_context(
                            self.pool.clone(),
                            session_id,
                            &binding_metadata,
                            &agent_input,
                            &session.client_type,
                            Some(&metadata),
                        )
                        .await?;
                    }
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
                        .map(|()| {
                            match client_spec.adapter.turn_context {
                                TurnContextBehavior::InternalApiClaim => {
                                    // Pending context for claim-based clients is stored before dispatch.
                                }
                                TurnContextBehavior::Disabled => {}
                            }
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
                                .ingest_event(ReportedEvent::new(
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
                                .ingest_event(ReportedEvent::new(
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
                        .ingest_event(ReportedEvent::new(
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
        let readiness = RuntimeReadinessService::new(self.pool.clone());
        if let Some(runtime_instance_id) = metadata["runtime_instance_id"].as_str() {
            readiness
                .wait_until_ready(session_id, client_type, runtime_instance_id)
                .await
        } else {
            readiness
                .wait_until_bound_and_ready(session_id, client_type)
                .await
                .map(|_| ())
        }
    }

    async fn runtime_binding_metadata(&self, session_id: &str) -> Result<Option<Value>> {
        SqliteRuntimeBindingRepository::new(self.pool.clone())
            .metadata(session_id)
            .await?
            .map(|metadata| serde_json::from_str(&metadata).map_err(Into::into))
            .transpose()
    }

    async fn required_tmux_pane_binding(&self, session_id: &str) -> Result<TmuxPaneBinding> {
        let row = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .tmux_pane_binding(session_id)
            .await?
            .ok_or_else(|| Error::Domain(format!("session {session_id} has no runtime binding")))?;
        match (row.socket_path, row.pane_id) {
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
