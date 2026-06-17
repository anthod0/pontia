use super::*;
use crate::{
    adapters::GenericTestAdapter,
    agent_clients::{TurnContextBehavior, get_client_spec},
    application::turns::write_client_current_turn_context,
};

impl SessionCommandService {
    pub(super) async fn dispatch_initial_generic_turn(
        &self,
        session_id: &str,
        turn_id: &str,
        client_type: &str,
        input: &str,
        runtime: &RuntimeStartResult,
    ) -> Result<()> {
        let agent_input = AgentInput {
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            input: input.to_string(),
        };
        let behavior = GenericTestAdapter::behavior();
        if behavior.write_current_turn_context {
            write_client_current_turn_context(&runtime.metadata, &agent_input, client_type, None)?;
        }
        self.runtime.submit_input(agent_input)?;
        if behavior.auto_start_turn {
            EventIngestService::new(self.pool.clone())
                .ingest_event(DomainEvent::new(
                    new_event_id().to_string(),
                    session_id.to_string(),
                    Some(turn_id.to_string()),
                    EventSource::AgentAdapter,
                    client_type.to_string(),
                    EventType::TurnStarted,
                    json!({}),
                ))
                .await?;
        }
        Ok(())
    }

    pub(super) async fn wait_and_dispatch_initial_tui_turn(
        &self,
        session_id: &str,
        turn_id: &str,
        client_type: &str,
        input: &str,
        runtime: &RuntimeStartResult,
    ) -> Result<()> {
        let runtime_instance_id = runtime.metadata["runtime_instance_id"]
            .as_str()
            .ok_or_else(|| {
                Error::Domain(format!(
                    "{client_type} runtime metadata missing runtime_instance_id"
                ))
            })?;
        let agent_input = AgentInput {
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            input: input.to_string(),
        };
        let turn_context = get_client_spec(client_type)
            .map(|spec| spec.turn_context)
            .ok_or_else(|| Error::Domain(format!("unsupported client_type: {client_type}")))?;
        let ingest = EventIngestService::new(self.pool.clone());
        let dispatch_result = RuntimeReadinessService::new(self.pool.clone())
            .wait_until_ready(session_id, client_type, runtime_instance_id)
            .await
            .and_then(|()| {
                if turn_context == TurnContextBehavior::CurrentTurnFile {
                    write_client_current_turn_context(
                        &runtime.metadata,
                        &agent_input,
                        client_type,
                        None,
                    )?;
                }
                Ok(())
            })
            .and_then(|()| {
                let socket_path = runtime.tmux_socket_path().ok_or_else(|| {
                    Error::Domain(format!(
                        "session {session_id} runtime cannot accept tasks: missing tmux socket path"
                    ))
                })?;
                let pane_id = runtime.tmux_pane_id().ok_or_else(|| {
                    Error::Domain(format!(
                        "session {session_id} runtime cannot accept tasks: missing tmux pane id"
                    ))
                })?;
                self.runtime
                    .dispatch_tui_turn(socket_path, pane_id, client_type, &agent_input)
            });

        match dispatch_result {
            Ok(()) => {
                if client_type != "pi" {
                    ingest
                        .ingest_event(DomainEvent::new(
                            new_event_id().to_string(),
                            session_id.to_string(),
                            Some(turn_id.to_string()),
                            EventSource::AgentAdapter,
                            client_type.to_string(),
                            EventType::TurnStarted,
                            json!({}),
                        ))
                        .await?;
                }
            }
            Err(error) => {
                ingest
                    .ingest_event(DomainEvent::new(
                        new_event_id().to_string(),
                        session_id.to_string(),
                        Some(turn_id.to_string()),
                        EventSource::RuntimeManager,
                        client_type.to_string(),
                        EventType::TurnFailed,
                        json!({ "failure": { "message": error.to_string() } }),
                    ))
                    .await?;
            }
        }
        Ok(())
    }
}
