use std::path::Path;

use crate::{AcpLaunch, peer::protocol_error};
use agent_client_protocol::{
    Agent, UntypedMessage,
    schema::ProtocolVersion,
    v1::{ConnectionTo, schema::*},
};
use pontia_agent_control::agent_clients::AgentClientModels;
use pontia_core::{Error, Result};
use serde::Deserialize;
use serde_json::json;

pub(crate) async fn list(config: &AcpLaunch, cwd: &Path) -> Result<AgentClientModels> {
    let (options_tx, mut options_rx) = tokio::sync::watch::channel(None);
    crate::stdio::run(
        config,
        cwd,
        "ACP model listing timed out",
        |streams| async move {
            agent_client_protocol::Client
                .v1()
                .on_receive_notification(
                    async move |notification: SessionNotification, _cx| {
                        if let SessionUpdate::ConfigOptionUpdate(update) = notification.update
                            && super::selector(
                                &update.config_options,
                                SessionConfigOptionCategory::Model,
                            )
                            .is_some()
                        {
                            options_tx.send_replace(Some((
                                notification.session_id,
                                update.config_options,
                            )));
                        }
                        Ok(())
                    },
                    agent_client_protocol::on_receive_notification!(),
                )
                .connect_with(streams, async |connection: ConnectionTo<Agent>| {
                    Ok(discover(&connection, cwd, &mut options_rx).await)
                })
                .await
        },
    )
    .await
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
    let initial = response.config_options.as_deref().unwrap_or_default();
    let models = if deferred && super::pending_catalog(initial) {
        let _ = tokio::time::timeout(
            super::MODEL_OPTIONS_WAIT,
            options.wait_for(|update| {
                update.as_ref().is_some_and(|(id, options)| {
                    id == &response.session_id && !super::pending_catalog(options)
                })
            }),
        )
        .await;
        let latest = options.borrow();
        let options = latest
            .as_ref()
            .filter(|(id, _)| id == &response.session_id)
            .map_or(initial, |(_, options)| options.as_slice());
        let models = super::catalog(options);
        if models.default_model.is_some() {
            Ok(models)
        } else {
            Err(Error::CapabilityUnavailable(
                "ACP model options did not arrive".into(),
            ))
        }
    } else {
        Ok(super::catalog(initial))
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
