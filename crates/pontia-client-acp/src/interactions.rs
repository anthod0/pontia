use std::{collections::HashMap, sync::Arc};

use agent_client_protocol::v1::{Responder, schema::*};
use pontia_agent_control::interactions::{
    ChoiceInteractionRequest, InteractionBlock, InteractionIntent, InteractionOption,
    InteractionService, InteractionState, authorization_digest, transient_storage_error,
};
use pontia_core::{Error, Result};
use serde_json::{Value, json};
use tokio::sync::{Mutex, oneshot};

use crate::{
    peer::{Peer, SessionState},
    transport::Receipts,
};

pub(crate) struct Permission {
    responder: Responder<RequestPermissionResponse>,
    options: HashMap<String, PermissionOptionId>,
    // Dropping the association also releases its native cancellation observer.
    _done: oneshot::Sender<()>,
}

const DIGEST: &str = "pontia.dev/authorization-digest";

// Inspect the original JSON before the SDK constructs its single-use responder.
// The typed JSON-RPC id and all params, including toolCall and native options,
// identify the authorization. Relay-owned metadata is added only after hashing.
pub(crate) async fn permission_frame(
    peer: &Peer,
    key: &str,
    frame: &mut Value,
) -> Result<Option<(String, bool)>> {
    let digest = authorization_digest(json!([frame["id"], frame["params"]]));
    let registering = peer
        .registering_permissions
        .lock()
        .unwrap()
        .get(key)
        .map(|done| done.subscribe());
    if let Some(mut registering) = registering {
        // The first frame was forwarded, but its asynchronous SDK handler may not yet have committed.
        let _ = registering.changed().await;
    }
    let registration = loop {
        match peer.interactions.registration(&peer.session, key).await {
            Err(error) if transient_storage_error(&error) => {
                tracing::warn!(session=%peer.session, %error, "ACP permission lookup will retry");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            result => break result?,
        }
    };
    if let Some((stored, original)) = registration {
        if original == digest {
            let ended = matches!(
                stored.state,
                InteractionState::Resolved | InteractionState::Expired
            );
            if ended
                || peer
                    .state
                    .lock()
                    .await
                    .permissions
                    .contains_key(&stored.interaction_id)
            {
                return Ok(Some((stored.interaction_id, ended)));
            }
        } else {
            let id = stored.interaction_id;
            let permission = peer.state.lock().await.permissions.remove(&id);
            if let Some(permission) = permission {
                let receipt = peer.receipts.response(permission.responder.id())?;
                permission
                    .responder
                    .respond(RequestPermissionResponse::new(
                        RequestPermissionOutcome::Cancelled,
                    ))
                    .map_err(crate::peer::protocol_error)?;
                let receipts = peer.receipts.clone();
                tokio::spawn(async move {
                    let _ = receipts.wait(receipt).await;
                });
                let service = peer.interactions.clone();
                let session = peer.session.clone();
                if let Err(error) = service.expire(&session, &id).await {
                    tracing::warn!(%session, %error, "ACP permission conflict expiry failed");
                    if transient_storage_error(&error) {
                        // Native approval has been revoked. Storage recovery must not stall the relay.
                        tokio::spawn(async move {
                            loop {
                                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                                match service.expire(&session, &id).await {
                                    Err(error) if transient_storage_error(&error) => continue,
                                    Err(error) => {
                                        tracing::warn!(%session, %error, "ACP permission expiry rejected")
                                    }
                                    Ok(()) => (),
                                }
                                break;
                            }
                        });
                    }
                }
            }
            return Err(Error::StateConflict(format!(
                "Conflicting ACP permission frame {key}"
            )));
        }
    }

    // An ill-formed request must reach the SDK's ordinary invalid-params handler.
    if serde_json::from_value::<RequestPermissionRequest>(frame["params"].clone()).is_ok() {
        let meta = &mut frame["params"]["_meta"];
        if !meta.is_object() {
            *meta = json!({});
        }
        meta[crate::transport::FRAME_KEY] = json!(key);
        meta[DIGEST] = json!(digest);
        peer.registering_permissions
            .lock()
            .unwrap()
            .insert(key.into(), tokio::sync::watch::channel(()).0);
    }
    Ok(None)
}

pub(crate) async fn register(
    service: &InteractionService,
    session: &str,
    runtime: &str,
    rpc: &Arc<pontia_agent_control::ipc::rpc::RpcPeer>,
    shared: &Arc<Mutex<SessionState>>,
    request: RequestPermissionRequest,
    responder: Responder<RequestPermissionResponse>,
) -> Result<()> {
    let mut state = shared.lock().await;
    let prompt = state.active.as_ref().filter(|_| {
        state.current && !state.closing && state.native.as_ref() == Some(&request.session_id)
    });
    let Some(prompt) = prompt else {
        responder
            .respond_with_error(agent_client_protocol::Error::invalid_params())
            .map_err(crate::peer::protocol_error)?;
        return Ok(());
    };
    if prompt.cancelled {
        responder
            .respond(RequestPermissionResponse::new(
                RequestPermissionOutcome::Cancelled,
            ))
            .map_err(crate::peer::protocol_error)?;
        return Ok(());
    }
    let turn = prompt.turn.clone();
    let Some((options, native)) = choice_options(request.options, state.agent.kiro) else {
        responder
            .respond_with_error(agent_client_protocol::Error::invalid_params())
            .map_err(crate::peer::protocol_error)?;
        return Ok(());
    };
    // Tool data is untrusted plain text, never markup or executable instructions.
    // Do not copy raw tool input/output (which can contain secrets) into the record.
    let description = request
        .tool_call
        .fields
        .title
        .filter(|title| !title.trim().is_empty());
    let registration = ChoiceInteractionRequest {
        report_key: crate::transport::frame_key(request.meta.as_ref())?,
        authorization_digest: request
            .meta
            .as_ref()
            .and_then(|meta| meta.get(DIGEST))
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Domain("Missing ACP permission digest".into()))?
            .into(),
        blocked: Some(InteractionBlock {
            key_prefix: "acp".into(),
            runtime_id: Some(runtime.into()),
            turn_id: Some(turn.clone()),
        }),
        session_id: session.into(),
        turn_id: Some(turn.clone()),
        title: "Tool permission".into(),
        description,
        options,
    };
    let interaction = loop {
        if responder.cancellation().is_cancelled()
            || !state.current
            || state.closing
            || !state
                .active
                .as_ref()
                .is_some_and(|p| p.turn == turn && !p.cancelled)
        {
            responder
                .respond(RequestPermissionResponse::new(
                    RequestPermissionOutcome::Cancelled,
                ))
                .map_err(crate::peer::protocol_error)?;
            return Ok(());
        }
        match service.register(registration.clone()).await {
            Ok(interaction) => break interaction,
            Err(error) if transient_storage_error(&error) => {
                drop(state);
                tracing::warn!(%session, %error, "ACP permission registration will retry");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                state = shared.lock().await;
            }
            Err(error) => {
                responder
                    .respond(RequestPermissionResponse::new(
                        RequestPermissionOutcome::Cancelled,
                    ))
                    .map_err(crate::peer::protocol_error)?;
                return Err(error);
            }
        }
    };
    if !matches!(
        interaction.state,
        InteractionState::Pending | InteractionState::Resolving
    ) {
        responder
            .respond(RequestPermissionResponse::new(
                RequestPermissionOutcome::Cancelled,
            ))
            .map_err(crate::peer::protocol_error)?;
        return Ok(());
    }
    let rpc = Arc::downgrade(rpc);
    let cancellation = responder.cancellation();
    let (done, completed) = oneshot::channel();
    let id = interaction.interaction_id;
    state.permissions.insert(
        id.clone(),
        Permission {
            responder,
            options: native,
            _done: done,
        },
    );
    let shared = Arc::downgrade(shared);
    let service = service.clone();
    let session = session.to_owned();
    tokio::spawn(async move {
        tokio::select! {
            _ = completed => return,
            _ = cancellation.cancelled() => {}
        }
        if rpc.upgrade().is_some_and(|rpc| !rpc.is_closed())
            && let Some(shared) = shared.upgrade()
        {
            let mut state = shared.lock().await;
            if state.permissions.remove(&id).is_some()
                && let Err(error) = service.expire(&session, &id).await
            {
                tracing::warn!(%session, %error, "ACP permission expiration failed");
            }
        }
    });
    Ok(())
}

fn choice_options(
    options: Vec<PermissionOption>,
    one_time_only: bool,
) -> Option<(Vec<InteractionOption>, HashMap<String, PermissionOptionId>)> {
    let mut choices = Vec::new();
    let mut native = HashMap::new();
    for option in options {
        let intent = match option.kind {
            PermissionOptionKind::AllowAlways | PermissionOptionKind::RejectAlways
                if one_time_only =>
            {
                continue;
            }
            PermissionOptionKind::AllowOnce | PermissionOptionKind::AllowAlways => {
                InteractionIntent::Positive
            }
            PermissionOptionKind::RejectOnce | PermissionOptionKind::RejectAlways => {
                InteractionIntent::Negative
            }
            _ => return None,
        };
        let key = option.option_id.to_string();
        if key.trim().is_empty() || option.name.trim().is_empty() || native.contains_key(&key) {
            return None;
        }
        native.insert(key.clone(), option.option_id);
        choices.push(InteractionOption {
            key,
            label: option.name,
            intent,
        });
    }
    (!choices.is_empty()).then_some((choices, native))
}

pub(crate) async fn resolve(peer: &Peer, id: &str, option: &str) -> Result<()> {
    let mut state = peer.state.lock().await;
    let receipts = peer.receipts.clone();
    let permission = state
        .permissions
        .get(id)
        .filter(|permission| state.current && !permission.responder.cancellation().is_cancelled())
        .ok_or_else(|| Error::CapabilityUnavailable("ACP permission expired".into()))?;
    let option_id = permission
        .options
        .get(option)
        .cloned()
        .ok_or_else(|| Error::Domain("Invalid ACP permission option".into()))?;
    let permission = state.permissions.remove(id).expect("checked permission");
    let key = receipts.response(permission.responder.id())?;
    permission
        .responder
        .respond(RequestPermissionResponse::new(
            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option_id)),
        ))
        .map_err(crate::peer::protocol_error)?;
    // The connection-bound responder has been consumed atomically. Reader
    // callbacks must remain free to take the state lock while the pipe drains.
    drop(state);
    receipts.wait(key).await
}

pub(crate) async fn expire(
    service: &InteractionService,
    session: &str,
    permissions: HashMap<String, Permission>,
    cancel: Option<&Receipts>,
) -> Result<()> {
    let mut first_error = None;
    for (id, permission) in permissions {
        let result = async {
            if let Some(receipts) = cancel
                && !permission.responder.cancellation().is_cancelled()
            {
                let key = receipts.response(permission.responder.id())?;
                permission
                    .responder
                    .respond(RequestPermissionResponse::new(
                        RequestPermissionOutcome::Cancelled,
                    ))
                    .map_err(crate::peer::protocol_error)?;
                receipts.wait(key).await?;
            }
            Ok(())
        }
        .await;
        // Always expire the record, including when native delivery cannot be confirmed.
        let expired = service.expire(session, &id).await;
        if let Err(error) = result.and(expired) {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

#[cfg(test)]
mod replay_tests;
