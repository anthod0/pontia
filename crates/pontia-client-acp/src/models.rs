pub(crate) mod probe;

use crate::peer::{Peer, SessionState, protocol_error};
use agent_client_protocol::v1::schema::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelect,
    SessionConfigSelectOption, SessionConfigSelectOptions, SetSessionConfigOptionRequest,
};
use pontia_agent_control::{
    EventIngestService, agent_clients::AgentClientModels, sessions::SessionModel,
};
use pontia_core::{Error, Result, domain::EventType};
use serde_json::json;

fn model_option(
    options: &[SessionConfigOption],
) -> Option<(&SessionConfigOption, &SessionConfigSelect)> {
    options.iter().find_map(|option| match &option.kind {
        SessionConfigKind::Select(select)
            if option.category == Some(SessionConfigOptionCategory::Model) =>
        {
            Some((option, select))
        }
        _ => None,
    })
}

pub(crate) fn current(options: &[SessionConfigOption]) -> Option<String> {
    model_option(options).map(|(_, select)| select.current_value.to_string())
}

fn values(select: &SessionConfigSelect) -> impl Iterator<Item = &SessionConfigSelectOption> {
    let (flat, groups) = match &select.options {
        SessionConfigSelectOptions::Ungrouped(options) => (options.as_slice(), &[][..]),
        SessionConfigSelectOptions::Grouped(groups) => (&[][..], groups.as_slice()),
        _ => (&[][..], &[][..]),
    };
    flat.iter()
        .chain(groups.iter().flat_map(|group| &group.options))
}

fn catalog(options: &[SessionConfigOption]) -> AgentClientModels {
    let Some((_, select)) = model_option(options) else {
        return AgentClientModels {
            models: Vec::new(),
            default_model: None,
        };
    };
    AgentClientModels {
        models: values(select)
            .map(|model| SessionModel {
                id: model.value.to_string(),
                name: model.name.clone(),
                description: model.description.clone().unwrap_or_default(),
            })
            .collect(),
        default_model: Some(select.current_value.to_string()),
    }
}

pub(crate) async fn select(peer: &std::sync::Arc<Peer>, model: &str, resuming: bool) -> Result<()> {
    let state = peer.state.lock().await;
    let Some((option, select)) = model_option(&state.config_options) else {
        // An Agent may restore its own model without offering a selector on load.
        if resuming {
            return Ok(());
        }
        return Err(Error::CapabilityUnavailable(
            "ACP Agent did not provide model options".into(),
        ));
    };
    if resuming && state.current_model.as_deref() == Some(model) {
        return Ok(());
    }
    if !values(select).any(|value| value.value.to_string() == model) {
        return Err(Error::Domain(format!("ACP model is unavailable: {model}")));
    }
    let request = peer
        .connection
        .prepare_request(SetSessionConfigOptionRequest::new(
            state.native.clone().expect("initialized ACP Session"),
            option.id.clone(),
            model,
        ));
    let key = serde_json::to_string(request.id())?;
    let result_peer = peer.clone();
    let model = model.to_owned();
    let (send, receive) = tokio::sync::oneshot::channel();
    request
        .on_receiving_result(async move |result| {
            let result = async {
                let response = result.map_err(protocol_error)?;
                let mut state = result_peer.state.lock().await;
                if !state.current {
                    return Err(Error::ControlUnknown(
                        "ACP connection closed during model selection".into(),
                    ));
                }
                replace(&mut state, response.config_options);
                if state.current_model.as_deref() != Some(&model) {
                    return Err(Error::Domain(format!(
                        "ACP Agent did not select model: {model}"
                    )));
                }
                result_peer.remember(&state, None).await
            }
            .await;
            let _ = send.send(result);
            Ok(())
        })
        .map_err(protocol_error)?;
    drop(state);
    peer.receipts.wait(key).await?;
    receive
        .await
        .map_err(|_| Error::ControlUnknown("ACP model selection response was lost".into()))?
}

pub(crate) fn replace(state: &mut SessionState, options: Vec<SessionConfigOption>) {
    state.current_model = current(&options);
    state.config_options = options;
}

pub(crate) async fn report(
    events: &EventIngestService,
    session: &str,
    runtime: &str,
    model: &str,
    key: String,
) -> Result<()> {
    crate::peer::fact(
        events,
        session,
        runtime,
        None,
        EventType::SessionModelUpdated,
        key,
        json!({"model":model}),
    )
    .await
}
