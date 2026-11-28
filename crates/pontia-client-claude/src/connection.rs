use crate::{Adapter, Input, Peer, peer::field, transport::Connection};
use pontia_agent_control::{
    AgentBindingService, AgentControl, PontiaEventType,
    ipc::{
        identity::{BindingConfirmation, RuntimeAttachment},
        rpc::{Requests, RpcPeer, RpcRequest},
    },
    runtime::ControlTarget,
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
                let code = match error {
                    Error::StateConflict(_)
                    | Error::NotFound(_)
                    | Error::Domain(_)
                    | Error::Serialization(_) => -32009,
                    _ => -32603,
                };
                let _ = rpc.reply_error(first.id, code, &error.to_string()).await;
                return Err(error);
            }
        };
        let mut sequence = 0;
        let serving = async {
            while let Some(request) = requests.recv().await {
                let result = async {
                    let _identity = self.connection_identity.lock().await;
                    self.validate_connection(&app, &peer, &connection).await?;
                    if request.method != "host.event" {
                        return Err(Error::Domain("Unknown Claude host method".into()));
                    }
                    let next = request.params["sequence"]
                        .as_u64()
                        .ok_or_else(crate::unavailable)?;
                    if request.params["host_id"] != connection.host_id || next == 0 {
                        return Err(Error::StateConflict(
                            "Claude host fact identity changed".into(),
                        ));
                    }
                    let result = peer
                        .event(
                            &app.event_ingest_service(),
                            &connection.host_id,
                            &request.params,
                            next <= sequence,
                        )
                        .await?;
                    if request.params["report_key"].is_string() {
                        sequence = sequence.max(next);
                    }
                    Ok(result)
                }
                .await;
                match result {
                    Ok(result) => rpc.reply(request.id, result).await?,
                    Err(error) => {
                        tracing::warn!(session=%peer.session, %error, "Claude host fact rejected");
                        let permission = matches!(
                            request
                                .params
                                .pointer("/event/kind")
                                .and_then(Value::as_str),
                            Some("interaction_requested" | "interaction_settled")
                        );
                        let code = match error {
                            Error::Domain(_) => -32602,
                            Error::StateConflict(_)
                            | Error::Conflict { .. }
                            | Error::Unsupported(_)
                            | Error::NotFound(_) => -32009,
                            Error::Unavailable(_) => -32603,
                            _ if permission
                                && !pontia_agent_control::interactions::transient_storage_error(
                                    &error,
                                ) =>
                            {
                                -32009
                            }
                            _ => -32603,
                        };
                        rpc.reply_error(request.id, code, &error.to_string())
                            .await?;
                    }
                }
            }
            Ok(())
        }
        .await;
        // Socket loss alone is not a runtime or execution exit.
        peer.unavailable(&connection, "control_connection_lost")
            .await;
        let mut shutdown = app.shutdown().subscribe();
        loop {
            if connection.process.observe() == ProcessObservation::Exited {
                peer.unavailable(&connection, "host_unavailable").await;
                app.event_ingest_service()
                    .ingest_runtime_observation_event(pontia_agent_control::PontiaEvent::new(
                        &peer.session,
                        None,
                        pontia_agent_control::PontiaEventSource::RuntimeManager,
                        "claude-agent",
                        PontiaEventType::SessionExited,
                        json!({
                            "reason":"host_exited",
                            "runtime_id":peer.runtime,
                            "role":"claude_sdk_host"
                        }),
                    ))
                    .await?;
                peer.runtime_event(
                    &app.event_ingest_service(),
                    PontiaEventType::RuntimeExited,
                    json!({"reason":"host_exited"}),
                )
                .await?;
                break;
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(20)) => {}
                _ = shutdown.wait_for(|stop| *stop) => break,
            }
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
        let _identity = self.connection_identity.lock().await;
        self.validate_attachment(app, &identity, &status, &connection)
            .await?;
        let mut peers = self.peers.lock().await;
        let peer = match peers.get(&identity.session_id) {
            Some(peer) if peer.runtime == identity.runtime_id => {
                if peer
                    .connection()
                    .is_ok_and(|current| !current.rpc.is_closed())
                {
                    return Err(Error::StateConflict(
                        "Claude runtime already has an active connection".into(),
                    ));
                }
                peer.clone()
            }
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
                    native_session_id: binding
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
            rpc.reply(
                first.id.clone(),
                json!({"session_id":identity.session_id,"runtime_id":identity.runtime_id}),
            )
            .await?;
            *peer.connection.write().unwrap() = Some(connection.clone());
        }
        app.inbox_commands().notify_available(&peer.session);
        Ok((peer, connection))
    }

    async fn validate_connection(
        &self,
        app: &AgentControl,
        peer: &Arc<Peer>,
        connection: &Arc<Connection>,
    ) -> Result<()> {
        if self.stopped.load(std::sync::atomic::Ordering::Acquire)
            || !self
                .peers
                .lock()
                .await
                .get(&peer.session)
                .is_some_and(|current| Arc::ptr_eq(current, peer))
            || !peer
                .connection()
                .is_ok_and(|current| Arc::ptr_eq(&current, connection))
            || connection.rpc.is_closed()
        {
            return Err(Error::StateConflict(
                "Claude host connection has been replaced".into(),
            ));
        }
        validate_runtime(app, &peer.session, &peer.runtime).await
    }

    async fn validate_attachment(
        &self,
        app: &AgentControl,
        identity: &RuntimeAttachment,
        status: &Value,
        connection: &Connection,
    ) -> Result<()> {
        if self.stopped.load(std::sync::atomic::Ordering::Acquire) {
            return Err(crate::unavailable());
        }
        let binding = attachment_binding(app, identity, status, connection).await?;
        validate_binding(app, identity, binding).await?;
        if identity.client_session_key.is_none() {
            sqlx::query("UPDATE session_runtimes SET process_fingerprint=? WHERE runtime_id=? AND session_id=? AND state='starting' AND process_fingerprint IS NULL AND EXISTS(SELECT 1 FROM sessions WHERE session_id=? AND client_type='claude-agent' AND state!='exited')")
                .bind(serde_json::to_string(&connection.process)?)
                .bind(&identity.runtime_id)
                .bind(&identity.session_id)
                .bind(&identity.session_id)
                .execute(&app.db())
                .await?;
        }
        let confirmed: Option<String> = sqlx::query_scalar(
            "SELECT process_fingerprint FROM session_runtimes WHERE runtime_id=? AND session_id=?",
        )
        .bind(&identity.runtime_id)
        .bind(&identity.session_id)
        .fetch_optional(&app.db())
        .await?;
        let confirmed = confirmed
            .as_deref()
            .map(serde_json::from_str::<ProcessFingerprint>)
            .transpose()?;
        if confirmed.as_ref() != Some(&connection.process) {
            return Err(Error::StateConflict(
                "Claude host connection does not belong to the confirmed process".into(),
            ));
        }
        let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM session_runtimes r JOIN sessions s USING(session_id) WHERE s.session_id=? AND r.runtime_id=? AND (s.state IN ('created','starting','idle','busy','waiting_for_interaction','interrupted') OR (s.state='error' AND r.state='starting')) AND r.state IN ('starting','running'))")
            .bind(&identity.session_id)
            .bind(&identity.runtime_id)
            .fetch_one(&app.db())
            .await?;
        if !active {
            return Err(Error::StateConflict(
                "Claude host connection does not match an active runtime".into(),
            ));
        }
        sqlx::query(
            "UPDATE sessions SET metadata=json_remove(metadata,'$.incompatible_control_runtime')
             WHERE session_id=? AND json_type(metadata,'$.incompatible_control_runtime') IS NOT NULL",
        )
        .bind(&identity.session_id)
        .execute(&app.db())
        .await?;
        Ok(())
    }
}

async fn attachment_binding<'a>(
    app: &AgentControl,
    identity: &'a RuntimeAttachment,
    status: &Value,
    connection: &Connection,
) -> Result<BindingConfirmation<'a>> {
    let Some(native) = identity.client_session_key.as_deref() else {
        return Ok(BindingConfirmation::Launch);
    };
    let pending = &status["pending_ready"];
    let sequence = pending["report_key"]
        .as_str()
        .unwrap_or_default()
        .strip_prefix(&format!("claude:{}:", connection.host_id))
        .and_then(|suffix| suffix.parse::<u64>().ok())
        .filter(|sequence| *sequence > 0);
    if sequence.is_some()
        && pending["native_session_id"] == native
        && status["native_session_id"] == native
    {
        let fingerprint: Option<String> = sqlx::query_scalar(
            "SELECT r.process_fingerprint FROM session_runtimes r JOIN sessions s USING(session_id)
             WHERE s.session_id=? AND s.client_type='claude-agent' AND s.state IN ('created','starting')
             AND r.runtime_id=? AND r.state='starting' AND r.process_fingerprint IS NOT NULL
             AND NOT EXISTS (SELECT 1 FROM agent_bindings b WHERE b.client_type='claude-agent'
                             AND (b.session_id=s.session_id OR b.client_session_key=?))",
        )
        .bind(&identity.session_id)
        .bind(&identity.runtime_id)
        .bind(native)
        .fetch_optional(&app.db())
        .await?;
        if fingerprint
            .as_deref()
            .map(serde_json::from_str::<ProcessFingerprint>)
            .transpose()?
            .as_ref()
            == Some(&connection.process)
        {
            return Ok(BindingConfirmation::Launch);
        }
    }
    Ok(BindingConfirmation::Native(native))
}

async fn validate_binding(
    app: &AgentControl,
    identity: &RuntimeAttachment,
    binding: BindingConfirmation<'_>,
) -> Result<()> {
    match binding {
        BindingConfirmation::Native(native) => {
            let binding = AgentBindingService::new(app.db())
                .binding_for_client_session("claude-agent", native)
                .await?
                .ok_or_else(|| Error::NotFound("Claude native binding not found".into()))?;
            if binding.session_id != identity.session_id {
                return Err(Error::StateConflict(
                    "Claude native Session does not match runtime identity".into(),
                ));
            }
        }
        BindingConfirmation::Launch => {
            let launching: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sessions s WHERE s.session_id=? AND s.client_type='claude-agent' AND EXISTS(SELECT 1 FROM session_runtimes r WHERE r.session_id=s.session_id AND r.runtime_id=? AND r.state='starting'))")
                .bind(&identity.session_id)
                .bind(&identity.runtime_id)
                .fetch_one(&app.db())
                .await?;
            if !launching {
                return Err(Error::StateConflict(
                    "Claude runtime must confirm its native binding".into(),
                ));
            }
        }
    }
    validate_runtime(app, &identity.session_id, &identity.runtime_id).await
}

async fn validate_runtime(app: &AgentControl, session: &str, runtime: &str) -> Result<()> {
    ControlTarget {
        session_id: session.into(),
        runtime_id: Some(runtime.into()),
        control_runtime_role: Some("claude_sdk_host"),
    }
    .validate(&app.db())
    .await
}
