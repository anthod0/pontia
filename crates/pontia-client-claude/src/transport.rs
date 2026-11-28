use crate::peer::Peer;
use pontia_agent_control::{
    DeliveryError, EventIngestService,
    ipc::rpc::{MAX_FRAME_BYTES, RpcPeer},
};
use pontia_core::{Error, Result};
use pontia_runtime::{ProcessFingerprint, ProcessObservation};
use serde_json::{Value, json};
use std::{
    io::{Seek, Write},
    path::Path,
    process::Stdio,
    sync::Arc,
    time::Duration,
};

const CONTROL_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) struct Connection {
    pub rpc: Arc<RpcPeer>,
    pub host_id: String,
    pub process: ProcessFingerprint,
}

pub(crate) async fn launch(
    peer: &Peer,
    root: &Path,
    script: &Path,
    environment: &std::collections::BTreeMap<String, String>,
    reasoning_effort: Option<&str>,
    events: &EventIngestService,
) -> Result<()> {
    let mode = pontia_agent_control::ControlQueryService::new(events.db())
        .session_mode(&peer.session)
        .await?;
    let mut bootstrap = json!({
        "session_id": peer.session,
        "runtime_id": peer.runtime,
        "pontia_home": root,
        "launch": {"cwd": peer.cwd, "mode": mode},
    });
    if let Some(binding) = pontia_agent_control::SessionInstructionsService::new(events.db())
        .session_instructions(&peer.session)
        .await?
        && let Some(prompt) = binding.get("system_prompt").and_then(Value::as_str)
    {
        bootstrap["system_prompt"] = json!(prompt);
    }
    if let Some(model) = &peer.model {
        bootstrap["model"] = json!(model);
    }
    if let Some(effort) = reasoning_effort {
        bootstrap["reasoning_effort"] = json!(effort);
    }
    if let Some(resume) = &peer.resume {
        bootstrap["resume"] = json!(resume);
    }
    if let Some(native) = &peer.native_session_id {
        bootstrap["native_session_id"] = json!(native);
    }
    let mut input = tempfile::tempfile()?;
    writeln!(input, "{bootstrap}")?;
    input.rewind()?;
    std::fs::create_dir_all(root.join("state"))?;
    let log = pontia_core::private_fs::append_log(&root.join("state/claude-host.log"))?;
    let mut command =
        pontia_runtime::detached_command(Path::new(&crate::setup::node()), "Pontia Claude host")?;
    let mut child = command
        .arg(script)
        .current_dir(&peer.cwd)
        .envs(environment)
        .env("PONTIA_HOME", root)
        .env("PONTIA_SESSION_ID", &peer.session)
        .env("PONTIA_RUNTIME_INSTANCE_ID", &peer.runtime)
        .env("CLAUDE_CONFIG_DIR", &peer.config_dir)
        .stdin(input)
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()?;
    let ready = tokio::time::timeout(CONTROL_TIMEOUT, async {
        loop {
            if peer.connection().is_ok() {
                return Ok(());
            }
            if child.try_wait()?.is_some() {
                return Err(Error::Unavailable(
                    "Claude Code host did not start (HOST_START_FAILED)".into(),
                ));
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        Err(Error::Unavailable(
            "Claude Code host startup timed out".into(),
        ))
    });
    if ready.is_err()
        && let Some(process) = ProcessFingerprint::capture(child.id())
    {
        let _ = pontia_runtime::GenericRuntimeManager.terminate_process(&process);
    }
    pontia_runtime::reap_detached_child(child);
    ready
}

impl Connection {
    pub async fn status(rpc: &RpcPeer) -> Result<Value> {
        let response = rpc
            .request("host.status", json!({}), MAX_FRAME_BYTES, false)
            .await?;
        response
            .get("result")
            .cloned()
            .ok_or_else(|| Error::ControlUnknown("Claude host status unavailable".into()))
    }

    pub async fn request(&self, request: Value) -> Result<Value> {
        let response = self
            .rpc
            .request("host.command", request, 1024 * 1024, false)
            .await?;
        let response = &response["result"];
        match response["status"].as_str() {
            Some("accepted") => Ok(response.clone()),
            Some("rejected") if response["reason"] == "input_busy" => Err(Error::Conflict {
                code: "input_busy",
                message: "Claude Code input is still active".into(),
            }),
            Some("rejected") => Err(Error::StateConflict(
                "Claude Code rejected control request".into(),
            )),
            Some("unsupported") => Err(Error::Unsupported(
                "Claude Code operation unsupported".into(),
            )),
            _ => Err(Error::ControlUnknown(
                "Claude Code operation unconfirmed".into(),
            )),
        }
    }

    pub async fn deliver(&self, request: Value) -> std::result::Result<Value, DeliveryError> {
        let response = self
            .rpc
            .request("host.command", request, 1024 * 1024, false)
            .await
            .map_err(DeliveryError::from)?;
        delivery_response(&response["result"])
    }

    pub async fn wait_exit(&self) -> Result<()> {
        tokio::time::timeout(CONTROL_TIMEOUT, async {
            while self.process.observe() != ProcessObservation::Exited {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .map_err(|_| Error::ControlUnknown("Claude Code host exit unconfirmed".into()))
    }
    pub fn stop(&self) {
        self.rpc.close();
    }

    pub async fn set_reasoning_effort(&self, reasoning_effort: &str) -> Result<()> {
        self.request(json!({"method":"set_reasoning_effort", "reasoning_effort":reasoning_effort}))
            .await
            .map(|_| ())
    }
}

fn delivery_response(response: &Value) -> std::result::Result<Value, DeliveryError> {
    let reason = || {
        response["reason"]
            .as_str()
            .unwrap_or("operation_unconfirmed")
            .to_owned()
    };
    match response["status"].as_str() {
        Some("accepted") => Ok(response.clone()),
        Some("unsupported") => Err(DeliveryError::Unsupported(reason())),
        Some("rejected") if response["reason"] == "execution_unavailable" => {
            Err(DeliveryError::Offline(reason()))
        }
        Some("rejected") => Err(DeliveryError::StateChanged(reason())),
        _ => Err(DeliveryError::Uncertain(reason())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn deliver(response: Value) -> std::result::Result<Value, DeliveryError> {
        let (client, server) = tokio::net::UnixStream::pair().unwrap();
        let (rpc, _) = RpcPeer::new(client);
        let (server, mut requests) = RpcPeer::new(server);
        let replying = tokio::spawn(async move {
            let request = requests.recv().await.unwrap();
            server.reply(request.id, response).await.unwrap();
        });
        let connection = Connection {
            rpc,
            host_id: "test-host".into(),
            process: ProcessFingerprint::capture(std::process::id()).unwrap(),
        };
        let result = connection.deliver(json!({"method":"interrupt"})).await;
        replying.await.unwrap();
        result
    }

    #[tokio::test]
    async fn delivery_boundary_preserves_the_four_delivery_outcomes() {
        assert!(deliver(json!({"status":"accepted"})).await.is_ok());
        assert!(matches!(
            deliver(json!({"status":"unsupported","reason":"unsupported_method"})).await,
            Err(DeliveryError::Unsupported(reason)) if reason == "unsupported_method"
        ));
        assert!(matches!(
            deliver(json!({"status":"rejected","reason":"execution_unavailable"})).await,
            Err(DeliveryError::Offline(reason)) if reason == "execution_unavailable"
        ));
        assert!(matches!(
            deliver(json!({"status":"rejected","reason":"no_matching_execution"})).await,
            Err(DeliveryError::StateChanged(reason)) if reason == "no_matching_execution"
        ));
        assert!(matches!(
            deliver(json!({"status":"unknown","reason":"operation_unconfirmed"})).await,
            Err(DeliveryError::Uncertain(reason)) if reason == "operation_unconfirmed"
        ));
    }
}
