use std::sync::Arc;

use agent_client_protocol::UntypedMessage;
use pontia_agent_control::{EventIngestService, control::InputReceipt, runtime::ControlTarget};
use pontia_core::{Error, Result};
use serde_json::json;
use tokio::sync::oneshot;

use crate::peer::{Peer, protocol_error};

fn prompt_required() -> Error {
    Error::Conflict {
        code: "steer_requires_prompt",
        message: "Input must wait for the current prompt to finish".into(),
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
    target.validate(&events.db()).await?;
    if !state.current || state.closing || !state.ready {
        return Err(Error::CapabilityUnavailable(
            "ACP connection unavailable".into(),
        ));
    }
    if !state.steer || state.active.as_ref().map(|prompt| prompt.turn.as_str()) != Some(turn) {
        return Err(prompt_required());
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
        .map_err(protocol_error)?,
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
            let result = result.map_err(protocol_error).and_then(|response| {
                match response["outcome"].as_str() {
                    Some("injected")
                        if state.current
                            && state
                                .active
                                .as_ref()
                                .is_some_and(|prompt| prompt.turn == turn) =>
                    {
                        crate::history::append(&mut state.history, "user", &input);
                        state
                            .active
                            .as_mut()
                            .expect("checked active prompt")
                            .history_end += input.len();
                        Ok(())
                    }
                    Some("promptRequired") => Err(prompt_required()),
                    Some("failed") => Err(Error::StateConflict("ACP steering was rejected".into())),
                    _ => Err(Error::ControlUnknown(
                        "ACP steering outcome could not be associated with the active Turn".into(),
                    )),
                }
            });
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
                result_peer.invalidate(&mut state);
            }
            let _ = send.send(result);
            Ok(())
        })
        .map_err(protocol_error)?;
    if let Err(error) = peer.receipts.wait(key).await {
        peer.invalidate(&mut state);
        return Err(error);
    }
    drop(state);
    receive
        .await
        .map_err(|_| Error::ControlUnknown("ACP steering response was lost".into()))??;
    Ok(InputReceipt {
        native_turn_id: None,
        runtime_id: target.runtime_id.clone(),
    })
}
