use pontia_application::{
    EventIngestService, ReportedFact,
    interactions::{ChoiceInteractionRequest, InteractionIntent, InteractionOption},
};
use pontia_core::{Error, Result, domain::EventType};
use serde_json::{Value, json};

use crate::peer::{Peer, State, field};

pub(crate) struct Permission {
    native: String,
    blocking: bool,
    resolving: bool,
}

pub(crate) async fn event(
    peer: &Peer,
    state: &mut State,
    events: &EventIngestService,
    host: &str,
    event: &Value,
) -> Result<()> {
    let native = field(event, "permission_id")?;
    match event["kind"].as_str() {
        Some("permission_requested") => {
            if state.closed
                || state.unavailable.is_some()
                || state.permissions.values().any(|p| p.native == native)
            {
                return Err(expired());
            }
            let blocking = event["blocking"].as_bool().ok_or_else(expired)?;
            let interaction = peer
                .interactions
                .register(ChoiceInteractionRequest {
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
                })
                .await?;
            let id = interaction.interaction_id;
            state.permissions.insert(
                id.clone(),
                Permission {
                    native: native.into(),
                    blocking,
                    resolving: false,
                },
            );
            if blocking {
                report(peer, events, host, &id, EventType::InteractionBlocked).await?;
            }
        }
        Some("permission_settled") => {
            let id = state
                .permissions
                .iter()
                .find(|(_, p)| p.native == native)
                .map(|(id, _)| id.clone())
                .ok_or_else(expired)?;
            let permission = state.permissions.get(&id).expect("located permission");
            match event["outcome"].as_str() {
                Some("expired") => peer.interactions.expire(&peer.session, &id).await?,
                Some("resolved") if permission.resolving => {}
                _ => return Err(expired()),
            }
            if permission.blocking {
                report(peer, events, host, &id, EventType::InteractionUnblocked).await?;
            }
            state.permissions.remove(&id);
        }
        _ => return Err(expired()),
    }
    Ok(())
}

async fn report(
    peer: &Peer,
    events: &EventIngestService,
    host: &str,
    id: &str,
    kind: EventType,
) -> Result<()> {
    events
        .report_fact(ReportedFact {
            session_id: peer.session.clone(),
            turn_id: None,
            report_key: Some(format!("claude:{host}:{id}:{kind}")),
            fact_type: kind,
            data: json!({"runtime_id":peer.runtime,"interaction_id":id}),
        })
        .await
        .map_err(|error| Error::Domain(error.to_string()))?;
    Ok(())
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
    })
    .await
    .map_err(|_| Error::ControlUnknown("Claude Agent approval observer stopped".into()))?
}

pub(crate) async fn expire_all(peer: &Peer, state: &mut State) {
    for (id, _) in std::mem::take(&mut state.permissions) {
        if let Err(error) = peer.interactions.expire(&peer.session, &id).await {
            tracing::warn!(session=%peer.session, %error, "Claude Agent approval expiration failed");
        }
    }
    // Losing the host/control channel is not an SDK unblock or a Turn result.
}

fn expired() -> Error {
    Error::CapabilityUnavailable("Claude Agent approval expired".into())
}
