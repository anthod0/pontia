use crate::{Adapter, Input, Peer, peer::field, transport::Connection};
use pontia_agent_control::{
    AgentBindingService, AgentControl, ClientControlChannel, PontiaEventType,
    ipc::{
        identity::RuntimeAttachment,
        rpc::{Requests, RpcPeer, RpcRequest},
    },
};
use pontia_core::{Error, Result};
use pontia_runtime::{ProcessFingerprint, ProcessObservation};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

impl Adapter {
    pub(crate) async fn serve_connection(
        &self,
        app: AgentControl,
        process_id: Option<u32>,
        rpc: Arc<RpcPeer>,
        first: RpcRequest,
        mut requests: Requests,
    ) -> Result<()> {
        let setup = self.attach_connection(&app, process_id, &rpc, &first).await;
        let (peer, connection) = match setup {
            Ok(value) => value,
            Err(error) => {
                let _ = rpc.reply_error(first.id, -32009, &error.to_string()).await;
                return Err(error);
            }
        };
        let channel: Arc<dyn ClientControlChannel> = connection.clone();
        let mut sequence = 0;
        let serving = async {
            while let Some(request) = requests.recv().await {
                let result = async {
                    let _guard = app.client_control().lock_identity().await;
                    app.client_control()
                        .validate_connection(&peer.session, &peer.runtime, &channel)
                        .await?;
                    if request.method != "host.event" {
                        return Err(Error::Domain("Unknown Claude host method".into()));
                    }
                    let next = request.params["sequence"]
                        .as_u64()
                        .ok_or_else(crate::unavailable)?;
                    if request.params["host_id"] != connection.host_id || next <= sequence {
                        return Err(Error::StateConflict(
                            "Claude host fact identity changed".into(),
                        ));
                    }
                    sequence = next;
                    peer.event(
                        &app.event_ingest_service(),
                        &connection.host_id,
                        &request.params["event"],
                    )
                    .await?;
                    Ok(json!({"accepted":true}))
                }
                .await;
                match result {
                    Ok(result) => rpc.reply(request.id, result).await?,
                    Err(error) => {
                        tracing::warn!(session=%peer.session, %error, "Claude host fact rejected");
                        rpc.reply_error(request.id, -32009, &error.to_string())
                            .await?;
                    }
                }
            }
            Ok(())
        }
        .await;
        // Socket loss alone is not a runtime or execution exit.
        if peer
            .connection()
            .is_ok_and(|current| Arc::ptr_eq(&current, &connection))
        {
            peer.unavailable("control_connection_lost").await;
        }
        for _ in 0..25 {
            if connection.process.observe() == ProcessObservation::Exited {
                peer.unavailable("host_unavailable").await;
                peer.runtime_event(
                    &app.event_ingest_service(),
                    PontiaEventType::RuntimeExited,
                    json!({"reason":"host_exited"}),
                )
                .await?;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        serving
    }

    async fn attach_connection(
        &self,
        app: &AgentControl,
        process_id: Option<u32>,
        rpc: &Arc<RpcPeer>,
        first: &RpcRequest,
    ) -> Result<(Arc<Peer>, Arc<Connection>)> {
        if first.method != "runtime.attach" {
            return Err(Error::Domain(
                "Claude host requires runtime attachment".into(),
            ));
        }
        let identity: RuntimeAttachment = serde_json::from_value(first.params.clone())?;
        let status = Connection::status(rpc).await?;
        let connection = Arc::new(Connection {
            rpc: rpc.clone(),
            host_id: field(&status, "host_id")?.into(),
            process: process_id
                .and_then(ProcessFingerprint::capture)
                .ok_or_else(crate::unavailable)?,
        });
        identity
            .attach(app, "claude-agent", connection.clone())
            .await?;
        let mut peers = self.peers.lock().await;
        let peer = match peers.get(&identity.session_id) {
            Some(peer) if peer.runtime == identity.runtime_id => peer.clone(),
            Some(_) => {
                return Err(Error::StateConflict(
                    "Claude runtime identity changed".into(),
                ));
            }
            None => {
                let binding = AgentBindingService::new(app.db())
                    .binding_for_session(&identity.session_id)
                    .await?;
                let launch = &status["launch"];
                let peer = Arc::new(Peer {
                    session: identity.session_id.clone(),
                    runtime: identity.runtime_id.clone(),
                    cwd: field(launch, "cwd")?.into(),
                    config_dir: binding
                        .as_ref()
                        .and_then(|binding| binding.metadata["claude_config_dir"].as_str())
                        .or_else(|| launch["config_dir"].as_str())
                        .ok_or_else(crate::unavailable)?
                        .into(),
                    model: launch["model"].as_str().map(str::to_owned),
                    resume: binding
                        .as_ref()
                        .map(|binding| binding.client_session_key.clone()),
                    state: Default::default(),
                    connection: Default::default(),
                    interactions: app.interactions(),
                });
                {
                    let mut state = peer.state.lock().await;
                    state.native = binding.map(|binding| binding.client_session_key);
                    let active: Option<(String, String)> = sqlx::query_as("SELECT t.turn_id,e.payload FROM turns t JOIN events e ON e.turn_id=t.turn_id WHERE t.session_id=? AND t.state IN ('queued','running') AND e.event_type='turn.started' ORDER BY e.rowid DESC LIMIT 1")
                            .bind(&peer.session).fetch_optional(&app.db()).await?;
                    if let Some((turn, payload)) = active {
                        let payload: Value = serde_json::from_str(&payload)?;
                        let id = field(&payload, "claude_input_id")?;
                        if status["input_id"] != id {
                            return Err(Error::StateConflict(
                                "Claude host active input does not match persisted Turn".into(),
                            ));
                        }
                        state.input = Some(Input {
                            id: id.into(),
                            turn: Some(turn),
                            inbox: None,
                            summary: String::new(),
                            sequence: 0,
                        });
                    }
                }
                peers.insert(peer.session.clone(), peer.clone());
                peer
            }
        };
        {
            let mut state = peer.state.lock().await;
            if status["native_session_id"]
                .as_str()
                .is_some_and(|native| state.native.as_deref().is_some_and(|bound| native != bound))
            {
                return Err(Error::StateConflict(
                    "Claude native identity changed".into(),
                ));
            }
            state.unavailable = status["unavailable"]
                .as_str()
                .filter(|reason| *reason != "control_connection_lost")
                .map(str::to_owned);
            state.closed = status["closed"] == true;
            state.closing = status["closing"] == true;
        }
        rpc.reply(
            first.id.clone(),
            json!({"session_id":identity.session_id,"runtime_id":identity.runtime_id}),
        )
        .await?;
        *peer.connection.write().unwrap() = Some(connection.clone());
        app.inbox_commands().notify_available(&peer.session);
        Ok((peer, connection))
    }
}
