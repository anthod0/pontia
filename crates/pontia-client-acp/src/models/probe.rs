use std::{path::Path, process::Stdio, time::Duration};

use agent_client_protocol::{
    Agent, ByteStreams, UntypedMessage,
    schema::ProtocolVersion,
    v1::{ConnectionTo, schema::*},
};
use pontia_agent_control::agent_clients::AgentClientModels;
use pontia_core::{Error, Result};
use serde::Deserialize;
use serde_json::json;
use tokio::process::Command;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::{AcpLaunch, peer::protocol_error};

pub(crate) async fn list(config: &AcpLaunch, cwd: &Path) -> Result<AgentClientModels> {
    let mut child = Command::new(&config.command)
        .args(&config.args)
        .envs(&config.env)
        .env_remove("PONTIA_SESSION_ID")
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()?;
    let writer = child.stdin.take().expect("piped stdin");
    let reader = child.stdout.take().expect("piped stdout");
    let operation = agent_client_protocol::Client.v1().connect_with(
        ByteStreams::new(writer.compat_write(), reader.compat()),
        async |connection: ConnectionTo<Agent>| Ok(discover(&connection, cwd).await),
    );
    let result = tokio::time::timeout(Duration::from_secs(30), operation).await;
    // Closing stdin lets agents shut down any runtime processes they own.
    match tokio::time::timeout(Duration::from_secs(2), child.wait()).await {
        Ok(status) => {
            status?;
        }
        Err(_) => child.kill().await?,
    }
    result
        .map_err(|_| Error::CapabilityUnavailable("ACP model listing timed out".into()))?
        .map_err(protocol_error)?
}

async fn discover(connection: &ConnectionTo<Agent>, cwd: &Path) -> Result<AgentClientModels> {
    // Keep draft capabilities that the SDK's stable InitializeResponse omits.
    let response = connection
        .send_request(
            UntypedMessage::new("initialize", InitializeRequest::new(ProtocolVersion::V1))
                .map_err(protocol_error)?,
        )
        .block_task()
        .await
        .map_err(protocol_error)?;
    let initialize: InitializeResponse = serde_json::from_value(response.clone())?;
    if initialize.protocol_version != ProtocolVersion::V1 {
        return Err(Error::CapabilityUnavailable(
            "ACP peer did not negotiate protocol v1".into(),
        ));
    }
    if response
        .pointer("/agentCapabilities/sessionCapabilities/options")
        .is_some_and(serde_json::Value::is_object)
    {
        let response = connection
            .send_request(
                UntypedMessage::new("session/options", json!({"cwd": cwd}))
                    .map_err(protocol_error)?,
            )
            .block_task()
            .await
            .map_err(protocol_error)?;
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Options {
            #[serde(default)]
            config_options: Option<Vec<SessionConfigOption>>,
        }
        let options: Options = serde_json::from_value(response)?;
        return Ok(super::catalog(
            options.config_options.as_deref().unwrap_or_default(),
        ));
    }
    if !response
        .pointer("/agentCapabilities/sessionCapabilities/close")
        .is_some_and(serde_json::Value::is_object)
    {
        return Err(Error::CapabilityUnavailable(
            "ACP model discovery requires sessionCapabilities.close".into(),
        ));
    }
    let response = connection
        .send_request(NewSessionRequest::new(cwd.to_path_buf()))
        .block_task()
        .await
        .map_err(protocol_error)?;
    let models = super::catalog(response.config_options.as_deref().unwrap_or_default());
    connection
        .send_request(CloseSessionRequest::new(response.session_id))
        .block_task()
        .await
        .map_err(protocol_error)?;
    Ok(models)
}
