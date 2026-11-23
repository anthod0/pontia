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
        .env_remove("PONTIA_RUNTIME_INSTANCE_ID")
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()?;
    let writer = child.stdin.take().expect("piped stdin");
    let reader = child.stdout.take().expect("piped stdout");
    let (options_tx, mut options_rx) = tokio::sync::watch::channel(None);
    let operation = agent_client_protocol::Client
        .v1()
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                if let SessionUpdate::ConfigOptionUpdate(update) = notification.update
                    && super::model_option(&update.config_options).is_some()
                {
                    options_tx.send_replace(Some((notification.session_id, update.config_options)));
                }
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_with(
            ByteStreams::new(writer.compat_write(), reader.compat()),
            async |connection: ConnectionTo<Agent>| {
                Ok(discover(&connection, cwd, &mut options_rx).await)
            },
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

async fn discover(
    connection: &ConnectionTo<Agent>,
    cwd: &Path,
    options: &mut tokio::sync::watch::Receiver<Option<(SessionId, Vec<SessionConfigOption>)>>,
) -> Result<AgentClientModels> {
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
    let capabilities = &initialize.agent_capabilities.session_capabilities;
    if capabilities.close.is_none() {
        return Err(Error::CapabilityUnavailable(
            "ACP model discovery requires sessionCapabilities.close".into(),
        ));
    }
    let deferred = super::deferred_options(&initialize.agent_capabilities);
    // Kiro lists even empty, closed Sessions. Only create one we can remove.
    if deferred && capabilities.delete.is_none() {
        return Err(Error::CapabilityUnavailable(
            "Kiro model discovery requires sessionCapabilities.delete".into(),
        ));
    }
    let response = connection
        .send_request(NewSessionRequest::new(cwd.to_path_buf()))
        .block_task()
        .await
        .map_err(protocol_error)?;
    let models = super::catalog(response.config_options.as_deref().unwrap_or_default());
    let models = if models.default_model.is_none() && deferred {
        match tokio::time::timeout(
            super::MODEL_OPTIONS_WAIT,
            options.wait_for(|update| {
                update
                    .as_ref()
                    .is_some_and(|(id, _)| id == &response.session_id)
            }),
        )
        .await
        {
            Ok(Ok(update)) => Ok(super::catalog(&update.as_ref().expect("matched Session").1)),
            _ => Err(Error::CapabilityUnavailable(
                "ACP model options did not arrive".into(),
            )),
        }
    } else {
        Ok(models)
    };
    connection
        .send_request(CloseSessionRequest::new(response.session_id.clone()))
        .block_task()
        .await
        .map_err(protocol_error)?;
    if capabilities.delete.is_some() {
        connection
            .send_request(DeleteSessionRequest::new(response.session_id))
            .block_task()
            .await
            .map_err(protocol_error)?;
    }
    models
}
