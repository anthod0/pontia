use pontia_agent_clients::{TurnContextBehavior, get_client_spec};
use pontia_core::{
    error::{Error, Result},
    ids::new_dispatch_id,
};
use pontia_runtime::{AgentInput, RuntimeStartResult};
use serde_json::json;

use super::SessionCommandService;
use crate::{
    EventIngestService, PontiaEvent, PontiaEventSource, PontiaEventType, RuntimeReadinessService,
    turns::store_client_current_turn_context,
};

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
        if client_type == "pi" {
            let runtime_instance_id = RuntimeReadinessService::new(self.pool.clone())
                .wait_until_bound_and_ready(session_id, client_type)
                .await?;
            return self
                .pi_control
                .as_ref()
                .ok_or_else(|| {
                    Error::CapabilityUnavailable("Pi control service is unavailable".into())
                })?
                .submit(session_id, &runtime_instance_id, input, None)
                .await;
        }
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
            Ok(_) if turn_context == TurnContextBehavior::InternalApiClaim => {
                store_client_current_turn_context(
                    self.pool.clone(),
                    session_id,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PiControlService, PublishPiControlEndpoint};
    use pontia_storage_sqlite::{connect_sqlite, run_migrations};
    use serde_json::Value;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::UnixListener,
    };

    #[tokio::test]
    async fn initial_pi_input_uses_the_shared_socket_after_ready() {
        let root = tempfile::Builder::new().prefix("pi-").tempdir().unwrap();
        let pool = connect_sqlite(&format!(
            "sqlite://{}",
            root.path().join("test.db").display()
        ))
        .await
        .unwrap();
        run_migrations(&pool).await.unwrap();
        sqlx::query("INSERT INTO sessions (session_id,client_type,state) VALUES ('sess_pi','pi','starting')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO runtime_bindings (session_id,runtime_kind,runtime_instance_id,binding_state) VALUES ('sess_pi','pi_tui','rtinst_pi','confirmed')").execute(&pool).await.unwrap();
        let path = root.path().join("s");
        let listener = UnixListener::bind(&path).unwrap();
        let control = PiControlService::new(pool.clone(), root.path().into());
        control
            .publish_endpoint(PublishPiControlEndpoint {
                session_id: "sess_pi".into(),
                runtime_instance_id: "rtinst_pi".into(),
                socket_path: path.display().to_string(),
                version: 1,
            })
            .await
            .unwrap();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut stream = BufReader::new(socket);
            for method in ["hello", "ping", "submit"] {
                let mut line = String::new();
                stream.read_line(&mut line).await.unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                assert_eq!(request["method"], method);
                let result = match method {
                    "hello" => json!({"session_id":"sess_pi","runtime_instance_id":"rtinst_pi"}),
                    "ping" => json!({"pong":true}),
                    _ => {
                        assert_eq!(request["input"], "initial input");
                        json!({"accepted":true})
                    }
                };
                let reply = json!({"version":1,"request_id":request["request_id"],"result":result});
                stream
                    .get_mut()
                    .write_all(format!("{reply}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });
        control.ping("sess_pi", "rtinst_pi").await.unwrap();
        let service =
            SessionCommandService::new(pool.clone(), root.path().into()).with_pi_control(control);
        let dispatch = tokio::spawn(async move {
            service
                .wait_and_dispatch_initial_tui_turn(
                    "sess_pi",
                    "dispatch_one",
                    "pi",
                    "initial input",
                    &RuntimeStartResult {
                        runtime_kind: "pi_tui".into(),
                        runtime_handle: "unused".into(),
                        capabilities: pontia_agent_clients::pi::CAPABILITIES,
                        metadata: json!({}),
                    },
                )
                .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!dispatch.is_finished());
        EventIngestService::new(pool.clone())
            .ingest_reported_event(pontia_core::domain::ReportedEvent::new(
                "evt_ready".into(),
                "sess_pi".into(),
                None,
                pontia_core::domain::EventSource::AgentClient,
                "pi".into(),
                pontia_core::domain::EventType::SessionReady,
                json!({"runtime_instance_id":"rtinst_pi"}),
            ))
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), dispatch)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        server.await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}
