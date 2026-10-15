use std::{collections::HashMap, sync::Arc};

use agent_client_protocol::{Responder, schema::v1::*};
use pontia_application::interactions::{
    ChoiceInteractionRequest, InteractionIntent, InteractionOption, InteractionService,
};
use pontia_core::{Error, Result};
use tokio::sync::{Mutex, oneshot};

use crate::{peer::SessionState, transport::Receipts};

pub(crate) struct Permission {
    responder: Responder<RequestPermissionResponse>,
    options: HashMap<String, PermissionOptionId>,
    // Dropping the association also releases its native cancellation observer.
    _done: oneshot::Sender<()>,
}

pub(crate) async fn register(
    service: &InteractionService,
    session: &str,
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
    let Some((options, native)) = choice_options(request.options) else {
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
    let registration = service
        .register(ChoiceInteractionRequest {
            session_id: session.into(),
            turn_id: Some(prompt.turn.clone()),
            title: "Tool permission".into(),
            description,
            options,
        })
        .await;
    let interaction = match registration {
        Ok(interaction) => interaction,
        Err(error) => {
            responder
                .respond_with_internal_error("Permission registration failed")
                .map_err(crate::peer::protocol_error)?;
            return Err(error);
        }
    };
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
        if let Some(shared) = shared.upgrade() {
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
) -> Option<(Vec<InteractionOption>, HashMap<String, PermissionOptionId>)> {
    if options.is_empty() {
        return None;
    }
    let mut choices = Vec::new();
    let mut native = HashMap::new();
    for option in options {
        let intent = match option.kind {
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
    Some((choices, native))
}

pub(crate) async fn resolve(
    shared: &Mutex<SessionState>,
    receipts: &Receipts,
    id: &str,
    option: &str,
) -> Result<()> {
    let mut state = shared.lock().await;
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
