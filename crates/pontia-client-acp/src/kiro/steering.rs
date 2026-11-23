use std::{collections::HashMap, sync::Arc, time::Duration};

use agent_client_protocol::{UntypedMessage, v1::schema::InitializeResponse};
use pontia_agent_control::{control::InputReceipt, runtime::ControlTarget};
use pontia_core::{Error, Result};
use serde_json::{Value, json};
use tokio::sync::{MutexGuard, oneshot};

use crate::{
    peer::{Peer, SessionState, protocol_error},
    relay::Observation,
};

const METHOD: &str = "_session/steer";
const TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) async fn steering_supported(
    peer: &Peer,
    initialize: &InitializeResponse,
) -> Result<bool> {
    let methods = initialize
        .agent_capabilities
        .meta
        .as_ref()
        .and_then(|meta| meta.get("kiro")?["extensionMethods"].as_array());
    if methods.is_some_and(|methods| methods.iter().any(|m| m == METHOD)) {
        return Ok(true);
    }
    // Kiro 2.31 implements this unadvertised extension. With no Session or
    // message its handler only validates parameters; unknown methods give a different error.
    let request = peer
        .connection
        .send_request(UntypedMessage::new(METHOD, json!({})).map_err(protocol_error)?);
    let key = serde_json::to_string(request.id())?;
    peer.receipts.wait(key).await?;
    let result = tokio::time::timeout(Duration::from_secs(3), request.block_task()).await;
    Ok(
        matches!(result, Ok(Err(e)) if e.data.as_ref().and_then(|v| v["details"].as_str()) == Some("_session/steer: missing or invalid sessionId")),
    )
}

pub(crate) async fn steer(
    peer: &Arc<Peer>,
    mut state: MutexGuard<'_, SessionState>,
    target: &ControlTarget,
    turn: &str,
    input: &str,
) -> Result<InputReceipt> {
    let request = peer.connection.prepare_request(
        UntypedMessage::new(METHOD, json!({"sessionId":state.native,"message":input}))
            .map_err(protocol_error)?,
    );
    let key = serde_json::to_string(request.id())?;
    let (send, receive) = oneshot::channel();
    let callback_key = key.clone();
    let shared = peer.state.clone();
    peer.receipts.steering_request(key.clone(), turn);
    request
        .on_receiving_result(async move |result| {
            let error = match result {
                Ok(value) if queued_id(&value).is_some() => return Ok(()),
                Ok(value) if value["queued"] == false => {
                    Error::StateConflict("Kiro rejected steering".into())
                }
                Ok(_) => Error::ControlUnknown("Invalid Kiro steering response".into()),
                Err(e) => protocol_error(e),
            };
            if let Some(waiter) = shared.lock().await.kiro_receipts.remove(&callback_key) {
                let _ = waiter.send(Err(error));
            }
            Ok(())
        })
        .map_err(protocol_error)?;
    state.kiro_receipts.insert(key.clone(), send);
    // Native replay positions the user message at submission, before the model reads it.
    state.steering_history_pending = true;
    state.refresh_capabilities();
    drop(state);
    let result = tokio::time::timeout(TIMEOUT, async {
        peer.receipts.wait(key.clone()).await?;
        receive
            .await
            .map_err(|_| Error::ControlUnknown("Kiro steering connection closed".into()))?
    })
    .await;
    peer.state.lock().await.kiro_receipts.remove(&key);
    result.map_err(|_| Error::ControlUnknown("Kiro steering injection timed out".into()))??;
    Ok(InputReceipt {
        turn_pending: false,
        native_turn_id: None,
        runtime_id: target.runtime_id.clone(),
    })
}

fn queued_id(result: &Value) -> Option<&str> {
    (result["queued"] == true)
        .then(|| result["messageId"].as_str())
        .flatten()
        .filter(|id| !id.is_empty())
}

// The relay observes receipts, injections and prompt completion in native wire order.
#[derive(Default)]
pub(crate) struct Steers {
    requests: HashMap<String, String>,
    queued: HashMap<String, (String, String)>,
}

impl Steers {
    pub(crate) fn register(&mut self, frame: &Value) -> Result<()> {
        let text = frame
            .pointer("/params/message")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Domain("Kiro steering requires text".into()))?;
        self.requests.insert(frame["id"].to_string(), text.into());
        Ok(())
    }

    pub(crate) fn observe(&mut self, context: &mut Value, frame: &Value) -> Option<Observation> {
        if frame.get("method").is_none() {
            let request = frame["id"].to_string();
            let text = self.requests.remove(&request)?;
            self.queued
                .insert(queued_id(&frame["result"])?.into(), (request, text));
            return None;
        }
        if frame["method"] != "session/update" || frame["params"]["sessionId"] != context["native"]
        {
            return None;
        }
        let info = super::info(&frame["params"]["update"])?;
        if info["kind"] != "steering_injected" || info["replay"] == true {
            return None;
        }
        let (request, text) = self.queued.remove(info["messageId"].as_str()?)?;
        let prompt = &mut context["active"];
        let turn = prompt["turn"].as_str()?.to_owned();
        let history_end = prompt["history_end"].as_u64()? as usize + text.len();
        prompt["history_end"] = json!(history_end);
        Some(Observation::KiroSteered {
            turn,
            request,
            text,
            history_end,
        })
    }
}

#[derive(Default)]
pub(crate) struct Notes {
    prefixes: Vec<String>,
    held: String,
    inside: bool,
}

impl Notes {
    fn include(&mut self, id: &str) -> bool {
        let prefix = format!("[STEERING {id}:");
        if self.prefixes.contains(&prefix) {
            return false;
        }
        self.prefixes.push(prefix);
        true
    }

    fn filter(&mut self, chunk: &str) -> Option<String> {
        if self.prefixes.is_empty() {
            return None;
        }
        let mut pending = std::mem::take(&mut self.held);
        pending.push_str(chunk);
        let mut rest = pending.as_str();
        let mut visible = String::new();
        loop {
            if self.inside {
                let Some((_, tail)) = rest.split_once(']') else {
                    break;
                };
                self.inside = false;
                rest = tail;
            }
            let Some((before, after)) = rest.split_once('[') else {
                visible.push_str(rest);
                break;
            };
            visible.push_str(before);
            rest = &rest[before.len()..];
            if let Some(prefix) = self.prefixes.iter().find(|p| rest.starts_with(p.as_str())) {
                rest = &rest[prefix.len()..];
                self.inside = true;
            } else if self.prefixes.iter().any(|p| p.starts_with(rest)) {
                self.held = rest.into();
                break;
            } else {
                visible.push('[');
                rest = after;
            }
        }
        Some(visible)
    }

    pub(crate) fn replay_input(
        &mut self,
        messages: &mut Vec<crate::history::Message>,
        update: &Value,
    ) -> bool {
        if update["sessionUpdate"] != "user_message_chunk"
            || update.pointer("/_meta/kiro/source").and_then(Value::as_str) != Some("steer")
        {
            return false;
        }
        let Some(text) = update.pointer("/content/text").and_then(Value::as_str) else {
            return false;
        };
        if text.is_empty() {
            return true;
        } // Kiro's steering-cleared boundary.
        if let Some(id) = update
            .pointer("/_meta/kiro/messageId")
            .and_then(Value::as_str)
            && self.include(id)
        {
            // An injection may immediately follow the original user message.
            messages.push(crate::history::Message {
                role: "user",
                text: text.into(),
                ..Default::default()
            });
            return true;
        }
        false
    }

    pub(crate) fn strip_complete(&mut self, text: &mut String) {
        if let Some(mut visible) = self.filter(text) {
            visible.push_str(&std::mem::take(&mut self.held));
            self.inside = false;
            *text = visible;
        }
    }

    /// Keep partial note delimiters in the Agent's relay while the controller reconnects.
    /// A literal suffix that never became a note precedes the native terminal frame.
    pub(crate) fn live(&mut self, context: &Value, frame: &mut Value) -> Option<Value> {
        if context["kiro_steer"] != true || context["active"].is_null() {
            return None;
        }
        let finished =
            frame.get("method").is_none() && frame["id"] == context["active"]["request_id"];
        if finished {
            let tail = std::mem::take(self).held;
            return (!tail.is_empty()).then(|| json!({"jsonrpc":"2.0","method":"session/update","params":{
                "sessionId":context["native"], "update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":tail}}
            }}));
        }
        if frame["method"] != "session/update" || frame["params"]["sessionId"] != context["native"]
        {
            return None;
        }
        let update = &mut frame["params"]["update"];
        if let Some(info) = super::info(update)
            && info["kind"] == "steering_injected"
            && info["replay"] != true
            && let Some(id) = info["messageId"].as_str()
        {
            self.include(id);
        } else if update["sessionUpdate"] == "agent_message_chunk"
            && let Some(text) = update.pointer("/content/text").and_then(Value::as_str)
            && let Some(visible) = self.filter(text)
        {
            update["content"]["text"] = visible.into();
        }
        None
    }
}
