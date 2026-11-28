use std::sync::Arc;

use agent_client_protocol::{
    UntypedMessage,
    v1::schema::{ContentBlock, PromptRequest, TextContent},
};
use pontia_agent_control::{EventIngestService, control::InputReceipt, runtime::ControlTarget};
use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;

use crate::peer::{Peer, protocol_error};

#[derive(Serialize, Deserialize)]
pub(crate) struct Interjection {
    pub request_id: Value,
    pub turn: String,
    pub prompt: usize,
    pub digest: String,
    pub shown: bool,
}

pub(crate) fn text_digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

pub(crate) fn prompt_required() -> Error {
    Error::Conflict {
        code: "steer_requires_prompt",
        message: "Input must wait for the current prompt to finish".into(),
    }
}

pub(crate) fn method_unavailable() -> Error {
    Error::Conflict {
        code: "steer_method_unavailable",
        message: "The client does not implement the negotiated steering method".into(),
    }
}

pub(crate) fn response_error(error: agent_client_protocol::Error) -> Error {
    if error.code == agent_client_protocol::ErrorCode::METHOD_NOT_FOUND {
        method_unavailable()
    } else {
        protocol_error(error)
    }
}

pub(crate) async fn send(
    peer: &Arc<Peer>,
    events: &EventIngestService,
    target: &ControlTarget,
    turn: &str,
    input: &str,
) -> Result<InputReceipt> {
    let mut state = peer.state.lock().await;
    events.store().validate_target(target).await?;
    if !state.current || state.closing || !state.ready {
        return Err(Error::Unavailable("ACP connection unavailable".into()));
    }
    if !state.capabilities.as_ref().is_some_and(|caps| caps.steer)
        || state.active.as_ref().map(|prompt| prompt.turn.as_str()) != Some(turn)
    {
        return Err(prompt_required());
    }
    if state.negotiated.kiro_steer {
        return crate::kiro::steer(peer, state, target, turn, input).await;
    }
    if state.agent.prompt_steering {
        let native = state.native.clone().ok_or_else(prompt_required)?;
        let request = peer.connection.prepare_request(PromptRequest::new(
            native,
            vec![ContentBlock::Text(TextContent::new(input))],
        ));
        let id = serde_json::to_value(request.id())?;
        let key = id.to_string();
        peer.receipts.steering_request(key.clone(), turn);
        let result_peer = peer.clone();
        let result_id = id.clone();
        request
            .on_receiving_result(async move |result| {
                if let Err(error) = crate::peer::finish(&result_peer, &result_id, result).await {
                    tracing::warn!(%result_id, %error, "ACP steering prompt failed");
                }
                Ok(())
            })
            .map_err(crate::peer::submission_error)?;
        if let Err(error) = peer.receipts.wait(key).await {
            if matches!(error, Error::ControlUnknown(_)) {
                peer.invalidate(&mut state, &error);
            }
            return Err(error);
        }
        let prompt = state.active.as_mut().expect("checked active prompt");
        prompt.request_id = id;
        prompt.history_end += input.len();
        if let Some(output) = events.live_output()
            && let Err(error) = crate::live_output::user_input(
                &output,
                &target.session_id,
                &peer.runtime,
                prompt,
                input,
            )
            .await
        {
            tracing::warn!(%turn, %error, "ACP live steering input rejected");
        }
        // agy may finish streaming its previous answer before consuming this input.
        // Only session/load gives the native position of the injected message.
        state.steered_inputs.push_str(input);
        state.steering_history_pending = true;
        state.refresh_capabilities();
        return Ok(InputReceipt {
            turn_pending: false,
            native_turn_id: None,
            runtime_id: target.runtime_id.clone(),
        });
    }
    if state.negotiated.interject {
        let protocol = state.agent.interjection.expect("negotiated protocol");
        let id = pontia_core::ids::new_dispatch_id().to_string();
        let request = peer.connection.prepare_request(
            UntypedMessage::new(
                protocol.method,
                json!({"sessionId":state.native,"interjectionId":id,"text":input}),
            )
            .map_err(crate::peer::submission_error)?,
        );
        let key = serde_json::to_string(request.id())?;
        let prompt = state
            .active
            .as_ref()
            .and_then(|p| p.prompt_index)
            .ok_or_else(prompt_required)?;
        state.interjections.insert(
            id.clone(),
            Interjection {
                request_id: serde_json::to_value(request.id())?,
                turn: turn.into(),
                digest: text_digest(input),
                prompt,
                shown: false,
            },
        );
        if let Err(error) = peer.remember(&state, None).await {
            state.interjections.remove(&id);
            return Err(error);
        }
        let result_peer = peer.clone();
        let (send, receive) = oneshot::channel();
        request
            .on_receiving_result(async move |result| {
                let outcome = match result {
                    Ok(value)
                        if value.pointer("/result/status").and_then(Value::as_str)
                            == Some("queued") =>
                    {
                        Ok(())
                    }
                    Err(error)
                        if error.code == agent_client_protocol::ErrorCode::METHOD_NOT_FOUND =>
                    {
                        let mut state = result_peer.state.lock().await;
                        state.negotiated.interject = false;
                        state.interjections.remove(&id);
                        state.refresh_capabilities();
                        result_peer
                            .remember(&state, None)
                            .await
                            .and(Err(method_unavailable()))
                    }
                    Err(error) => Err(response_error(error)),
                    Ok(_) => Err(Error::ControlUnknown(
                        "ACP interjection receipt has unknown status".into(),
                    )),
                };
                let _ = send.send(outcome);
                Ok(())
            })
            .map_err(crate::peer::submission_error)?;
        peer.receipts.wait(key).await?;
        drop(state);
        tokio::time::timeout(std::time::Duration::from_secs(30), receive)
            .await
            .map_err(|_| Error::ControlUnknown("ACP interjection receipt timed out".into()))?
            .map_err(|_| Error::ControlUnknown("ACP interjection response was lost".into()))??;
        return Ok(InputReceipt {
            turn_pending: true,
            native_turn_id: None,
            runtime_id: target.runtime_id.clone(),
        });
    }
    let request = peer.connection.prepare_request(
        UntypedMessage::new(
            "_session/steering",
            json!({
                "sessionId": state.native,
                "prompt": [{"type": "text", "text": input}],
                "_meta": {"steering": {"idleBehavior": "promptRequired"}}
            }),
        )
        .map_err(crate::peer::submission_error)?,
    );
    let key = serde_json::to_string(request.id())?;
    let result_peer = peer.clone();
    let output = events.live_output();
    let session = target.session_id.clone();
    let runtime = peer.runtime.clone();
    let input = input.to_owned();
    let turn = turn.to_owned();
    let (send, receive) = oneshot::channel();
    request
        .on_receiving_result(async move |result| {
            let mut state = result_peer.state.lock().await;
            let result = match result {
                Err(error) if error.code == agent_client_protocol::ErrorCode::METHOD_NOT_FOUND => {
                    state.negotiated.steer = false;
                    state.refresh_capabilities();
                    result_peer
                        .remember(&state, None)
                        .await
                        .and(Err(method_unavailable()))
                }
                result => result.map_err(response_error).and_then(|response| {
                    match response["outcome"].as_str() {
                        Some("injected")
                            if state.current
                                && state
                                    .active
                                    .as_ref()
                                    .is_some_and(|prompt| prompt.turn == turn) =>
                        {
                            if state.history_error.is_none() {
                                crate::history::append(&mut state.history, "user", &input);
                            }
                            state
                                .active
                                .as_mut()
                                .expect("checked active prompt")
                                .history_end += input.len();
                            Ok(())
                        }
                        Some("promptRequired") => Err(prompt_required()),
                        Some("failed") => {
                            Err(Error::StateConflict("ACP steering was rejected".into()))
                        }
                        _ => Err(Error::ControlUnknown(
                            "ACP steering outcome could not be associated with the active Turn"
                                .into(),
                        )),
                    }
                }),
            };
            if result.is_ok()
                && let Some(output) = &output
            {
                let prompt = state.active.as_mut().expect("checked active prompt");
                // Live presentation is best effort; the injected input is already confirmed.
                if let Err(error) =
                    crate::live_output::user_input(output, &session, &runtime, prompt, &input).await
                {
                    tracing::warn!(%session, %error, "ACP live steering input rejected");
                }
            }
            if matches!(result, Err(Error::ControlUnknown(_))) {
                result_peer.invalidate(
                    &mut state,
                    result.as_ref().expect_err("checked control error"),
                );
            }
            let result = match result {
                Ok(()) => result_peer.remember(&state, None).await,
                Err(error) => Err(error),
            };
            let _ = send.send(result);
            Ok(())
        })
        .map_err(crate::peer::submission_error)?;
    if let Err(error) = peer.receipts.wait(key).await {
        peer.invalidate(&mut state, &error);
        return Err(error);
    }
    drop(state);
    receive
        .await
        .map_err(|_| Error::ControlUnknown("ACP steering response was lost".into()))??;
    Ok(InputReceipt {
        turn_pending: false,
        native_turn_id: None,
        runtime_id: target.runtime_id.clone(),
    })
}

impl Peer {
    pub(crate) async fn reconcile_interjections(
        &self,
        state: &mut crate::peer::SessionState,
    ) -> Result<()> {
        state.interjections.clear();
        self.remember(state, None).await
    }

    pub(crate) async fn interjection_echo(&self, method: &str, params: &Value) -> Result<()> {
        let mut state = self.state.lock().await;
        if !state.current
            || !state.agent.interjection.is_some_and(|p| p.echo == method)
            || state.native.as_ref().map(|s| s.0.as_ref()) != params["sessionId"].as_str()
        {
            return Ok(());
        }
        let Some(id) = params["interjectionId"].as_str() else {
            return Ok(());
        };
        let Some(entry) = state.interjections.get_mut(id) else {
            return Ok(());
        };
        let Some(text) = params["text"].as_str() else {
            return Ok(());
        };
        if entry.shown || text_digest(text) != entry.digest {
            return Ok(());
        }
        entry.shown = true;
        let turn = entry.turn.clone();
        if let Some(prompt) = state.active.as_mut().filter(|p| p.turn == turn)
            && let Some(output) = self.events.live_output()
        {
            crate::live_output::user_input(&output, &self.session, &self.runtime, prompt, text)
                .await?;
        }
        self.remember(&state, None).await
    }
}
