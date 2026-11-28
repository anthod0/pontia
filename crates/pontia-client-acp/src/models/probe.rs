use std::{path::Path, time::Duration};
use tokio::time::{Instant, timeout};

use crate::{AcpLaunch, peer::protocol_error};
use agent_client_protocol::{
    Agent, UntypedMessage,
    schema::ProtocolVersion,
    v1::{ConnectionTo, schema::*},
};
use pontia_agent_control::agent_clients::AgentClientModels;
use pontia_core::{Error, Result};
use serde::Deserialize;
use serde_json::{Value, json};

pub(crate) async fn list(config: &AcpLaunch, cwd: &Path) -> Result<AgentClientModels> {
    // Leave time within the stdio deadline for restoring and closing the probe session.
    let switch_deadline = Instant::now() + Duration::from_secs(24);
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
                    Ok(discover(&connection, cwd, &mut options_rx, switch_deadline).await)
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
    switch_deadline: Instant,
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
        return Err(Error::Unsupported(
            "ACP peer did not negotiate protocol v1".into(),
        ));
    }
    let meta = initialize.agent_capabilities.meta.as_ref();
    let grok = meta.is_some_and(|meta| meta.contains_key("x.ai/capabilities"));
    let deferred = super::deferred_options(&initialize.agent_capabilities);
    let rotate = deferred || meta.is_some_and(|meta| meta.contains_key("antigravity"));
    let capabilities = &initialize.agent_capabilities.session_capabilities;
    if response
        .pointer("/agentCapabilities/sessionCapabilities/options")
        .is_some_and(Value::is_object)
        && !(rotate && capabilities.close.is_some() && (!deferred || capabilities.delete.is_some()))
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
        let options = Options::deserialize(&response)?;
        let mut models = super::catalog(options.config_options.as_deref().unwrap_or_default());
        if grok {
            super::grok::read_efforts(&mut models, &response);
        }
        return Ok(models);
    }
    if capabilities.close.is_none() {
        return Err(Error::Unsupported(
            "ACP model discovery requires sessionCapabilities.close".into(),
        ));
    }
    // Kiro lists even empty, closed Sessions. Only create one we can remove.
    if deferred && capabilities.delete.is_none() {
        return Err(Error::Unsupported(
            "Kiro model discovery requires sessionCapabilities.delete".into(),
        ));
    }
    // The stable SDK omits Grok's models.availableModels from NewSessionResponse.
    let raw = connection
        .send_request(
            UntypedMessage::new("session/new", NewSessionRequest::new(cwd.to_path_buf()))
                .map_err(protocol_error)?,
        )
        .block_task()
        .await
        .map_err(protocol_error)?;
    let response = NewSessionResponse::deserialize(&raw)?;
    let mut initial = response.config_options.unwrap_or_default();
    if deferred && super::pending_catalog(&initial) {
        let _ = tokio::time::timeout(
            super::MODEL_OPTIONS_WAIT,
            options.wait_for(|update| {
                update.as_ref().is_some_and(|(id, options)| {
                    id == &response.session_id && !super::pending_catalog(options)
                })
            }),
        )
        .await;
        if let Some((id, latest)) = options.borrow().as_ref()
            && id == &response.session_id
        {
            initial.clone_from(latest);
        }
    }
    let mut models = super::catalog(&initial);
    if grok {
        super::grok::read_efforts(&mut models, &raw);
    } else if rotate
        && let Some((option, _)) = super::selector(&initial, SessionConfigOptionCategory::Model)
    {
        probe_efforts(
            connection,
            &response.session_id,
            &option.id,
            &mut models,
            switch_deadline,
        )
        .await;
    }
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
    if deferred && models.default_model.is_none() {
        return Err(Error::Unavailable(
            "ACP model options did not arrive".into(),
        ));
    }
    Ok(models)
}

async fn probe_efforts(
    connection: &ConnectionTo<Agent>,
    session: &SessionId,
    selector: &SessionConfigId,
    models: &mut AgentClientModels,
    deadline: Instant,
) {
    let Some(original) = models.default_model.as_deref() else {
        return;
    };
    let switch = |value: &str| {
        connection
            .send_request(SetSessionConfigOptionRequest::new(
                session.clone(),
                selector.clone(),
                value,
            ))
            .block_task()
    };
    let mut attempted = false;
    let candidates: Vec<_> = models
        .models
        .iter_mut()
        .filter(|model| model.id != original)
        .collect();
    let count = candidates.len();
    for (index, model) in candidates.into_iter().enumerate() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let budget = (remaining / (count - index) as u32).min(Duration::from_secs(2));
        attempted = true;
        if let Ok(Ok(response)) = timeout(budget, switch(&model.id)).await
            && super::current(&response.config_options).as_ref() == Some(&model.id)
        {
            super::read_efforts(model, &response.config_options);
        }
    }
    if attempted {
        let _ = timeout(Duration::from_secs(2), switch(original)).await;
    }
}
