use crate::peer::Peer;
use pontia_agent_control::{
    ClientControlChannel, ClientControlOperation, EventIngestService,
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
                return Err(Error::CapabilityUnavailable(
                    "Claude Code host did not start (HOST_START_FAILED)".into(),
                ));
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        Err(Error::CapabilityUnavailable(
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
            Some("unsupported") => Err(Error::CapabilityUnavailable(
                "Claude Code operation unsupported".into(),
            )),
            _ => Err(Error::ControlUnknown(
                "Claude Code operation unconfirmed".into(),
            )),
        }
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
}

impl ClientControlChannel for Connection {
    fn process_id(&self) -> Option<u32> {
        Some(self.process.pid)
    }
    fn available(&self) -> bool {
        !self.rpc.is_closed()
    }
    fn invalidate(&self) {
        self.stop();
    }
    fn ping(&self) -> ClientControlOperation<'_> {
        Box::pin(async { Self::status(&self.rpc).await.map(|_| ()) })
    }
    fn interrupt(&self) -> ClientControlOperation<'_> {
        Box::pin(async {
            let status = Self::status(&self.rpc).await?;
            self.request(json!({"method":"interrupt", "input_id":status["input_id"]}))
                .await
                .map(|_| ())
        })
    }
    fn shutdown(&self) -> ClientControlOperation<'_> {
        Box::pin(async { self.request(json!({"method":"close"})).await.map(|_| ()) })
    }
    fn replay<'a>(&'a self, _: &'a str) -> ClientControlOperation<'a> {
        Box::pin(async { Err(crate::unsupported()) })
    }
    fn submit<'a>(&'a self, _: &'a str, _: Option<&'a str>) -> ClientControlOperation<'a> {
        Box::pin(async { Err(crate::unsupported()) })
    }
}
