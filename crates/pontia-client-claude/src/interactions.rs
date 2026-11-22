use pontia_agent_control::interactions::{
    ChoiceInteractionRequest, InteractionBlock, InteractionIntent, InteractionOption,
    InteractionState,
};
use pontia_core::{Error, Result};
use serde_json::{Value, json};

use crate::peer::{Peer, State, field};

pub(crate) struct Permission {
    native: String,
    resolving: bool,
}

pub(crate) async fn event(
    peer: &Peer,
    state: &mut State,
    host: &str,
    params: &Value,
) -> Result<Value> {
    let event = &params["event"];
    let native = field(event, "permission_id")?;
    match event["kind"].as_str() {
        Some("permission_requested") => {
            let blocking = event["blocking"].as_bool().ok_or_else(expired)?;
            let key = field(params, "report_key")?;
            if key != format!("claude:{host}:{native}") {
                return Err(Error::StateConflict(
                    "Claude permission key does not match request".into(),
                ));
            }
            let request = ChoiceInteractionRequest {
                report_key: key.into(),
                authorization_digest: field(event, "authorization_digest")?.into(),
                blocked: blocking.then(|| InteractionBlock {
                    key_prefix: format!("claude:{host}"),
                    runtime_id: Some(peer.runtime.clone()),
                    turn_id: None,
                }),
                session_id: peer.session.clone(),
                // canUseTool identifies a callback, not a user input. In particular,
                // background work must never be assigned to the latest Inbox Turn.
                turn_id: None,
                title: "Tool permission".into(),
                description: Some(field(event, "tool")?.chars().take(256).collect()),
                options: vec![
                    InteractionOption {
                        key: "allow".into(),
                        label: "Allow once".into(),
                        intent: InteractionIntent::Positive,
                    },
                    InteractionOption {
                        key: "deny".into(),
                        label: "Deny once".into(),
                        intent: InteractionIntent::Negative,
                    },
                ],
            };
            let interaction = {
                match peer.interactions.register(request).await {
                    Ok(stored) => stored,
                    Err(error @ Error::StateConflict(_)) => {
                        if let Some((stored, _)) =
                            peer.interactions.registration(&peer.session, key).await?
                        {
                            peer.interactions
                                .expire(&peer.session, &stored.interaction_id)
                                .await?;
                            if let Some(permission) =
                                state.permissions.get_mut(&stored.interaction_id)
                            {
                                permission.resolving = true;
                            }
                        }
                        return Err(error);
                    }
                    Err(error) => return Err(error),
                }
            };
            if state.closed || state.unavailable.is_some() {
                peer.interactions
                    .expire(&peer.session, &interaction.interaction_id)
                    .await?;
                return Ok(
                    json!({"accepted":true,"interaction_id":interaction.interaction_id,"ended":true}),
                );
            }
            if matches!(
                interaction.state,
                InteractionState::Pending | InteractionState::Resolving
            ) {
                state
                    .permissions
                    .entry(interaction.interaction_id.clone())
                    .or_insert_with(|| Permission {
                        native: native.into(),
                        resolving: false,
                    });
            }
            return Ok(
                json!({"accepted": true, "interaction_id": interaction.interaction_id,
                "ended": matches!(interaction.state, InteractionState::Resolved | InteractionState::Expired)}),
            );
        }
        Some("permission_settled") => {
            let registration = field(params, "registration_key")?;
            let key = field(params, "report_key")?;
            let outcome = field(event, "outcome")?;
            if registration != format!("claude:{host}:{native}")
                || key != format!("{registration}:settled")
                || !matches!(outcome, "resolved" | "expired")
            {
                return Err(Error::StateConflict(
                    "Invalid Claude permission settlement".into(),
                ));
            }
            let id = peer
                .interactions
                .settle(
                    &peer.session,
                    registration,
                    field(event, "authorization_digest")?,
                    key,
                    outcome,
                )
                .await?;
            state.permissions.remove(&id);
        }
        _ => return Err(expired()),
    }
    Ok(json!({"accepted": true}))
}

pub(crate) async fn resolve(peer: &Peer, id: &str, option: &str) -> Result<()> {
    let native = {
        let mut state = peer.state.lock().await;
        if state.closed
            || state.closing
            || state.unavailable.is_some()
            || !matches!(option, "allow" | "deny")
        {
            return Err(expired());
        }
        let permission = state
            .permissions
            .get_mut(id)
            .filter(|p| !p.resolving)
            .ok_or_else(expired)?;
        permission.resolving = true;
        permission.native.clone()
    };
    let connection = peer.connection()?.clone();
    let option = option.to_owned();
    // Once claimed, delivery is observed independently of the HTTP caller. Never
    // replay a choice, and never hold the state lock while the reader processes facts.
    tokio::spawn(async move {
        connection
            .request(json!({"method":"permission","permission_id":native,"option":option}))
            .await
            .map(|_| ())
    })
    .await
    .map_err(|_| Error::ControlUnknown("Claude Code approval observer stopped".into()))?
}

pub(crate) async fn expire_all(peer: &Peer, state: &mut State) {
    for (id, _) in std::mem::take(&mut state.permissions) {
        if let Err(error) = peer.interactions.expire(&peer.session, &id).await {
            tracing::warn!(session=%peer.session, %error, "Claude Code approval expiration failed");
        }
    }
    // Losing the host/control channel is not an SDK unblock or a Turn result.
}

fn expired() -> Error {
    Error::CapabilityUnavailable("Claude Code approval expired".into())
}
