pub(crate) mod probe;

use crate::peer::{Peer, SessionState, protocol_error};
use agent_client_protocol::v1::schema::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory as Category,
    SessionConfigSelect, SessionConfigSelectOption, SessionConfigSelectOptions,
    SetSessionConfigOptionRequest,
};
use pontia_agent_control::{
    EventIngestService,
    agent_clients::AgentClientModels,
    sessions::{ReasoningEffortOption, SessionModel},
};
use pontia_core::{Error, Result, domain::EventType};
use serde_json::json;

const MODEL_OPTIONS_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

pub(crate) fn deferred_options(
    capabilities: &agent_client_protocol::v1::schema::AgentCapabilities,
) -> bool {
    capabilities
        .meta
        .as_ref()
        .is_some_and(|meta| meta.contains_key("kiro"))
}

fn selector(
    options: &[SessionConfigOption],
    category: Category,
) -> Option<(&SessionConfigOption, &SessionConfigSelect)> {
    options.iter().find_map(|option| match &option.kind {
        SessionConfigKind::Select(select) if option.category.as_ref() == Some(&category) => {
            Some((option, select))
        }
        _ => None,
    })
}

pub(crate) fn current(options: &[SessionConfigOption]) -> Option<String> {
    selector(options, Category::Model).map(|(_, select)| select.current_value.to_string())
}

pub(crate) fn effort(options: &[SessionConfigOption]) -> Option<String> {
    selector(options, Category::ThoughtLevel).map(|(_, select)| select.current_value.to_string())
}

// Kiro can publish just the current model before its model catalog arrives.
fn pending_catalog(options: &[SessionConfigOption]) -> bool {
    let Some((_, model)) = selector(options, Category::Model) else {
        return true;
    };
    selector(options, Category::ThoughtLevel).is_none() && values(model).count() <= 1
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
    let Some((_, select)) = selector(options, Category::Model) else {
        return AgentClientModels {
            models: Vec::new(),
            default_model: None,
        };
    };
    let thought_level = selector(options, Category::ThoughtLevel).map(|(_, select)| select);
    AgentClientModels {
        models: values(select)
            .map(|model| {
                let effort = thought_level.filter(|_| model.value == select.current_value);
                SessionModel {
                    reasoning_efforts: effort
                        .into_iter()
                        .flat_map(values)
                        .map(|level| ReasoningEffortOption {
                            id: level.value.to_string(),
                            name: level.name.clone(),
                            description: level.description.clone().unwrap_or_default(),
                        })
                        .collect(),
                    default_reasoning_effort: effort.map(|level| level.current_value.to_string()),
                    id: model.value.to_string(),
                    name: model.name.clone(),
                    description: model.description.clone().unwrap_or_default(),
                }
            })
            .collect(),
        default_model: Some(select.current_value.to_string()),
    }
}

pub(crate) async fn select(
    peer: &std::sync::Arc<Peer>,
    model: Option<&str>,
    reasoning_effort: Option<&str>,
    resuming: bool,
) -> Result<()> {
    let pending = {
        let state = peer.state.lock().await;
        (state.negotiated.deferred_models && pending_catalog(&state.config_options))
            .then(|| state.model_options_available.subscribe())
    };
    if let Some(mut available) = pending {
        // A single-model Agent can legitimately have no thought-level selector.
        let _ = tokio::time::timeout(MODEL_OPTIONS_WAIT, available.wait_for(|complete| *complete))
            .await;
        if model.is_some() && current(&peer.state.lock().await.config_options).is_none() {
            return Err(Error::CapabilityUnavailable(
                "ACP model options did not arrive".into(),
            ));
        }
    }
    if let Some(model) = model {
        if let Some(profile) = peer.policy.profile {
            profile.check_model(peer, Some(model)).await?;
        }
        set(peer, Category::Model, model, resuming).await?;
    }
    if let Some(effort) = reasoning_effort {
        set(peer, Category::ThoughtLevel, effort, resuming).await?;
    }
    Ok(())
}

async fn set(
    peer: &std::sync::Arc<Peer>,
    category: Category,
    value: &str,
    resuming: bool,
) -> Result<()> {
    let state = peer.state.lock().await;
    let is_model = category == Category::Model;
    let Some((option, select)) = selector(&state.config_options, category.clone()) else {
        if resuming {
            return Ok(());
        }
        return Err(Error::CapabilityUnavailable(format!(
            "ACP Agent did not provide a {category:?} selector"
        )));
    };
    if (resuming || is_model && peer.policy.profile.is_some())
        && select.current_value.0.as_ref() == value
    {
        return Ok(());
    }
    if is_model && !values(select).any(|entry| entry.value.0.as_ref() == value) {
        return Err(Error::Domain(format!("ACP model is unavailable: {value}")));
    }
    let native = state.native.clone().expect("initialized ACP Session");
    let request = peer
        .connection
        .prepare_request(SetSessionConfigOptionRequest::new(
            native.clone(),
            option.id.clone(),
            value,
        ));
    let key = serde_json::to_string(request.id())?;
    let result_peer = peer.clone();
    let value = value.to_owned();
    let (send, receive) = tokio::sync::oneshot::channel();
    request
        .on_receiving_result(async move |result| {
            let result = async {
                let response = result.map_err(protocol_error)?;
                let mut state = result_peer.state.lock().await;
                if !state.current {
                    return Err(Error::ControlUnknown(
                        "ACP connection closed during option selection".into(),
                    ));
                }
                replace(&mut state, response.config_options);
                if is_model && state.current_model.as_deref() != Some(&value) {
                    return Err(Error::Domain(format!(
                        "ACP Agent did not select model: {value}"
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
        .map_err(|_| Error::ControlUnknown("ACP option selection response was lost".into()))??;
    if is_model && let Some(profile) = peer.policy.profile {
        profile.verify(peer, &native).await?;
    }
    Ok(())
}

pub(crate) fn replace(state: &mut SessionState, options: Vec<SessionConfigOption>) {
    let model = current(&options);
    if state.current_model != model {
        state.verified_model = None;
    }
    if state.negotiated.deferred_models {
        state
            .model_options_available
            .send_replace(!pending_catalog(&options));
    }
    state.current_model = model;
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
    .map(|_| ())
}

pub(crate) async fn report_effort(
    events: &EventIngestService,
    session: &str,
    runtime: &str,
    effort: Option<String>,
    key: String,
) -> Result<()> {
    crate::peer::fact(
        events,
        session,
        runtime,
        None,
        EventType::SessionReasoningEffortUpdated,
        key,
        json!({"reasoning_effort":effort}),
    )
    .await
    .map(|_| ())
}
