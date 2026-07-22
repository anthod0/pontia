use super::*;
use pontia_agent_clients::{TurnContextBehavior, get_client_spec};

use crate::turns::store_client_current_turn_context;

impl SessionCommandService {
    pub(super) async fn dispatch_initial_generic_turn(
        &self,
        session_id: &str,
        client_type: &str,
        input: &str,
    ) -> Result<()> {
        let agent_input = AgentInput {
            session_id: session_id.to_string(),
            dispatch_id: new_dispatch_id().to_string(),
            input: input.to_string(),
        };
        self.runtime.submit_input(client_type, agent_input)?;
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
        let agent_input = AgentInput {
            session_id: session_id.to_string(),
            dispatch_id: new_dispatch_id().to_string(),
            input: input.to_string(),
        };
        let turn_context = get_client_spec(client_type)
            .map(|spec| spec.adapter.turn_context)
            .ok_or_else(|| Error::Domain(format!("unsupported client_type: {client_type}")))?;
        let ingest = EventIngestService::new(self.pool.clone());
        let readiness = RuntimeReadinessService::new(self.pool.clone())
            .wait_until_bound_and_ready(session_id, client_type)
            .await;
        let dispatch_result = match readiness {
            Ok(runtime_instance_id) if turn_context == TurnContextBehavior::InternalApiClaim => {
                let mut metadata = runtime.metadata.clone();
                metadata["runtime_instance_id"] = json!(runtime_instance_id);
                store_client_current_turn_context(
                    self.pool.clone(),
                    session_id,
                    &metadata,
                    &agent_input,
                    client_type,
                    None,
                )
                .await
            }
            Ok(_) => Ok(()),
            Err(error) => Err(error),
        }
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

        let client_spec = get_client_spec(client_type)
            .ok_or_else(|| Error::Domain(format!("unsupported client_type: {client_type}")))?;
        match dispatch_result {
            Ok(()) => {}
            Err(error) if client_spec.owns_initial_tmux_turn() => return Err(error),
            Err(error) => {
                ingest
                    .ingest_pontia_event(PontiaEvent::new(
                        session_id.to_string(),
                        Some(turn_id.to_string()),
                        PontiaEventSource::RuntimeManager,
                        client_type.to_string(),
                        PontiaEventType::TurnDispatchFailed,
                        json!({ "failure": { "message": error.to_string() } }),
                    ))
                    .await?;
            }
        }
        Ok(())
    }
}
