use crate::{
    adapter::AcpAdapter,
    peer::{Peer, Prompt, SessionState},
    transport::Relay,
};
use agent_client_protocol::v1::schema::{SessionConfigOption, SessionId, SystemPrompt};
use pontia_agent_control::{
    AgentControl, EventIngestService,
    ipc::{
        identity::RuntimeAttachment,
        rpc::{Requests, RpcPeer, RpcRequest},
    },
};
use pontia_core::{Error, Result};
use pontia_runtime::ProcessFingerprint;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::{Seek, Write},
    path::Path,
    sync::Arc,
    time::Duration,
};

#[derive(Serialize, Deserialize)]
struct Context {
    ready: bool,
    blocked: bool,
    native: Option<SessionId>,
    steer: bool,
    #[serde(default)]
    interject: bool,
    #[serde(default)]
    compact_command: bool,
    compaction: Option<crate::compaction::Pending>,
    system_prompt: Option<SystemPrompt>,
    current_model: Option<String>,
    #[serde(default)]
    model_windows: std::collections::BTreeMap<String, u64>,
    config_options: Vec<SessionConfigOption>,
    active: Option<Prompt>,
    promoted: Option<String>,
    #[serde(default)]
    interjections: std::collections::HashMap<String, crate::steering::Interjection>,
}

pub(crate) fn context(state: &SessionState) -> Value {
    json!({"ready":state.ready,"blocked":state.blocked,"native":state.native,"steer":state.negotiated.steer,"interject":state.negotiated.interject,"protocol":state.agent.interjection,"compact_command":state.negotiated.compact_command,"compaction":state.compaction,
        "compaction_events":state.agent.compaction_events(), "compaction_protocol":state.agent.compaction,
        "model_windows":state.model_windows,
        "system_prompt":state.system_prompt,"current_model":state.current_model,"config_options":state.config_options,"promoted":state.promoted,"interjections":state.interjections,"active":state.active})
}

impl Peer {
    pub fn remember<'a>(
        &'a self,
        state: &SessionState,
        binding: Option<&str>,
    ) -> pontia_agent_control::client_contract::ClientOperation<'a, ()> {
        let params = json!({
            "client_session_key":binding,
            "context": context(state),
        });
        Box::pin(async move { self.relay.call("relay.state", params).await.map(|_| ()) })
    }

    pub async fn terminate(&self) -> Result<()> {
        self.relay.terminate().await?;
        self.stop().await;
        crate::peer::runtime_event(
            &self.events,
            &self.session,
            &self.runtime,
            pontia_agent_control::PontiaEventType::RuntimeExited,
            json!({"reason":"agent_exited"}),
        )
        .await
    }
}

pub(crate) async fn restore(
    events: &EventIngestService,
    session: &str,
    state: &mut SessionState,
    value: &Value,
) -> Result<()> {
    let context: Context = serde_json::from_value(value.clone())?;
    state.ready = context.ready;
    state.native = context.native;
    state.negotiated.steer = context.steer;
    state.negotiated.interject = context.interject && state.agent.interjection.is_some();
    state.negotiated.compact_command = context.compact_command;
    state.promoted = context.promoted;
    state.interjections = context.interjections;
    state.compaction = context.compaction;
    state.system_prompt = context.system_prompt;
    state.current_model = context.current_model;
    state.config_options = context.config_options;
    state.model_windows = context.model_windows;
    state.blocked = context.blocked;
    if let Some(mut prompt) = context.active {
        let status: Option<String> = sqlx::query_scalar("SELECT state FROM turns WHERE session_id=? AND turn_id=? AND state IN ('queued','running')")
            .bind(session).bind(&prompt.turn).fetch_optional(&events.db()).await?;
        if let Some(status) = status {
            prompt.started = status == "running";
            prompt.sequence = 0;
            prompt.raw_results.clear();
            prompt.thought = None;
            state.active = Some(prompt);
            state.blocked = false;
        }
    }
    // Native history is reloaded when idle; reattachment never reloads or resends an active prompt.
    state.history_error =
        Some("ACP history requires a fresh session/load after reattachment".into());
    state.refresh_capabilities();
    Ok(())
}

impl AcpAdapter {
    pub(crate) async fn attach(
        &self,
        app: AgentControl,
        process_id: Option<u32>,
        rpc: Arc<RpcPeer>,
        first: RpcRequest,
        requests: Requests,
    ) -> Result<()> {
        let setup = async {
            if first.method != "runtime.attach" {
                return Err(Error::Domain(
                    "ACP relay requires runtime attachment".into(),
                ));
            }
            let identity: RuntimeAttachment = serde_json::from_value(first.params.clone())?;
            let relay = Arc::new(Relay {
                rpc: rpc.clone(),
                process: process_id
                    .and_then(ProcessFingerprint::capture)
                    .ok_or_else(|| {
                        Error::ControlUnknown("ACP relay process is unavailable".into())
                    })?,
            });
            let status = relay.call("relay.status", json!({})).await?;
            identity.attach(&app, "acp", relay.clone()).await?;
            let previous = self.peers.lock().await.get(&identity.session_id).cloned();
            if let Some(previous) = previous {
                previous.stop().await;
            }
            let cwd = status["cwd"]
                .as_str()
                .ok_or_else(|| Error::Domain("ACP relay omitted its launch directory".into()))?;
            let binding = identity
                .client_session_key
                .as_deref()
                .map(crate::identity::decode)
                .transpose()?;
            let agent_key = match &binding {
                Some(id) => id[0].clone(),
                None => sqlx::query_scalar::<_, String>(
                    "SELECT json_extract(metadata, '$.acp_agent_id') FROM sessions WHERE session_id=?",
                ).bind(&identity.session_id).fetch_one(&app.db()).await?,
            };
            let definition = self.config.agents.get(&agent_key);
            let policy = app.queries().session_execution_policy(&identity.session_id).await?;
            let policy = crate::restrictions::SessionPolicy::resolve(definition.and_then(|agent| agent.restricted_execution), &policy)?;
            let peer = Peer::connect(
                &app,
                &identity,
                Path::new(cwd),
                relay,
                requests,
                status["context"].clone(),
                crate::peer::PeerOwner {
                    agent: crate::capabilities::AgentBounds::of(&agent_key, definition),
                    policy,
                    peers: Arc::downgrade(&self.peers),
                    reconnect: self.reconnect.clone(),
                },
            )
            .await?;
            if let Some(native) = binding {
                let state = peer.state.lock().await;
                if state.native.as_ref().map(ToString::to_string).as_deref()
                    != Some(native[1].as_str())
                {
                    return Err(Error::StateConflict(
                        "ACP relay native binding changed".into(),
                    ));
                }
            }
            rpc.reply(
                first.id.clone(),
                json!({"session_id":identity.session_id,"runtime_id":identity.runtime_id}),
            )
            .await?;
            let mut attached = peer.attached.clone();
            attached.wait_for(|ready| *ready).await.map_err(|_| {
                Error::ControlUnknown("ACP relay did not confirm attachment".into())
            })?;
            self.peers
                .lock()
                .await
                .insert(identity.session_id.clone(), peer.clone());
            if status["setup_status"] == "confirmed" && peer.state.lock().await.active.is_none() {
                let history = peer.clone();
                tokio::spawn(async move {
                    let _ = history.load().await;
                });
            }
            app.inbox_commands().notify_available(&identity.session_id);
            Ok(peer)
        }
        .await;
        let peer = match setup {
            Ok(peer) => peer,
            Err(error) => {
                let code = match error {
                    Error::StateConflict(_)
                    | Error::Domain(_)
                    | Error::Serialization(_)
                    | Error::NotFound(_) => -32009,
                    _ => -32603,
                };
                let _ = rpc.reply_error(first.id, code, &error.to_string()).await;
                return Err(error);
            }
        };
        let mut done = peer.done.clone();
        let _ = done.wait_for(|finished| *finished).await;
        Ok(())
    }

    pub(crate) async fn launch(
        &self,
        root: &Path,
        session: &str,
        runtime: &str,
        cwd: &Path,
        config: crate::AcpLaunch,
    ) -> Result<Arc<Peer>> {
        let executable = self
            .relay_executable
            .clone()
            .map(Ok)
            .unwrap_or_else(std::env::current_exe)?;
        let bootstrap = crate::relay::Bootstrap {
            pontia_home: root.into(),
            session_id: session.into(),
            runtime_id: runtime.into(),
            client_session_key: None,
            cwd: cwd.into(),
            command: config.command.into(),
            args: config.args,
            environment: config.env,
        };
        let mut input = tempfile::tempfile()?;
        serde_json::to_writer(&mut input, &bootstrap)?;
        input.flush()?;
        input.rewind()?;
        std::fs::create_dir_all(root.join("state"))?;
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("state/acp-relay.log"))?;
        let mut command = pontia_runtime::detached_command(&executable, "Pontia stdio relay")?;
        let mut child = command
            .arg("--stdio-relay")
            .stdin(input)
            .stdout(std::process::Stdio::null())
            .stderr(log)
            .spawn()?;
        let started = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if let Some(peer) = self.peers.lock().await.get(session).cloned() {
                    return Ok(peer);
                }
                if child.try_wait()?.is_some() {
                    return Err(Error::CapabilityUnavailable(
                        "ACP relay startup failed".into(),
                    ));
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            Err(Error::CapabilityUnavailable(
                "ACP relay startup timed out".into(),
            ))
        });
        if started.is_err()
            && let Some(process) = ProcessFingerprint::capture(child.id())
        {
            let _ = pontia_runtime::GenericRuntimeManager.terminate_process(&process);
        }
        pontia_runtime::reap_detached_child(child);
        started
    }
}
