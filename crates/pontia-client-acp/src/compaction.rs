use crate::peer::{Peer, SessionState, protocol_error};
use crate::relay::Observation;
use agent_client_protocol::UntypedMessage;
use pontia_agent_control::client_contract::raw_transcripts::{TimelineItem, TurnTimelineItem};
use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Deserialize, Serialize)]
pub(crate) struct Pending {
    pub operation: String,
    pub request_id: Value,
}

pub(crate) async fn request(peer: &Peer, operation: String) -> Result<()> {
    let mut state = peer.state.lock().await;
    if !state
        .capabilities
        .as_ref()
        .is_some_and(|caps| caps.manual_compaction)
    {
        return Err(Error::CapabilityUnavailable(
            "Manual compaction is unavailable".into(),
        ));
    }
    let request = peer.connection.prepare_request(
        UntypedMessage::new(
            state
                .agent
                .compaction
                .expect("negotiated compaction protocol")
                .method(),
            json!({"sessionId": state.native}),
        )
        .map_err(protocol_error)?,
    );
    let key = serde_json::to_string(request.id())?;
    state.compaction = Some(Pending {
        operation,
        request_id: serde_json::to_value(request.id())?,
    });
    state.refresh_capabilities();
    // The detached relay correlates native completion even if pontiad disconnects.
    let remember = peer.remember(&state, None);
    let sent = async move {
        remember.await?;
        request
            .on_receiving_result(async |_| Ok(()))
            .map_err(protocol_error)?;
        peer.receipts.wait(key).await
    }
    .await;
    if let Err(error) = &sent {
        peer.invalidate(&mut state, error);
    }
    sent
}

impl Peer {
    pub(crate) async fn finish_compaction(
        &self,
        state: &mut SessionState,
        operation: &str,
    ) -> Result<()> {
        if state
            .compaction
            .as_ref()
            .is_some_and(|pending| pending.operation == operation)
        {
            state.history_dirty = true;
            state.compaction = None;
            state.refresh_capabilities();
            self.remember(state, None).await?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct Checkpoint {
    pub offset: usize,
    pub id: String,
    pub created_at: String,
}

impl Checkpoint {
    pub(crate) fn item(&self) -> TurnTimelineItem {
        TurnTimelineItem {
            turn_id: None,
            item: TimelineItem {
                item_id: format!(
                    "acp:{:020}:checkpoint:{}:{}",
                    self.offset, self.created_at, self.id
                ),
                kind: "context_compaction".into(),
                role: "system".into(),
                raw_kind: Some("compaction_checkpoint".into()),
                status: Some("completed".into()),
                occurred_at: Some(self.created_at.clone()),
                content_preview: String::new(),
                title: None,
                call_id: None,
                managed_tool_use: None,
            },
        }
    }
}

pub(crate) fn replay(state: &mut SessionState, params: &Value) {
    if state.agent.compaction == Some(crate::CompactionProtocol::Grok)
        && state.current
        && state
            .native
            .as_ref()
            .is_some_and(|native| params["sessionId"].as_str() == Some(native.0.as_ref()))
        && let Some(replay) = &mut state.replay
    {
        replay.checkpoint(params);
    }
}

// Grok uses different methods for live facts and session/load replay. Replay is
// read by the timeline and must never settle a current operation.
pub(crate) fn observe(context: &mut Value, frame: &Value) -> Option<Observation> {
    let params = &frame["params"];
    let pending = &context["compaction"];
    let manual = pending["operation"].as_str();
    let observation = if frame.get("method").is_none() {
        let operation = manual?;
        if frame["id"] != pending["request_id"] {
            return None;
        }
        // The native terminal notification normally arrives before the response.
        // The command cannot confirm a checkpoint from an empty response alone.
        Observation::CompactionFailed {
            operation: operation.into(),
            manual: true,
            aborted: false,
            message: frame["error"]["message"]
                .as_str()
                .unwrap_or("Grok returned without confirming compaction")
                .into(),
        }
    } else {
        if frame["method"] != "_x.ai/session_notification"
            || params["sessionId"] != context["native"]
            || params["_meta"]["isReplay"] == true
        {
            return None;
        }
        let update = &params["update"];
        let kind = update["sessionUpdate"].as_str()?;
        if !matches!(
            kind,
            "auto_compact_completed" | "auto_compact_failed" | "auto_compact_cancelled"
        ) {
            return None;
        }
        let native_id = params["_meta"]["eventId"].as_str()?;
        let operation = manual
            .map(str::to_owned)
            .unwrap_or_else(|| format!("grok:{native_id}"));
        if kind == "auto_compact_completed" {
            Observation::Compacted {
                operation,
                manual: manual.is_some(),
                native_compaction_id: Some(native_id.into()),
                tokens_before: update["tokens_before"].as_u64(),
                tokens_after: update["tokens_after"].as_u64(),
            }
        } else {
            let aborted = kind == "auto_compact_cancelled";
            Observation::CompactionFailed {
                operation,
                manual: manual.is_some(),
                aborted,
                message: update["error"]
                    .as_str()
                    .unwrap_or(if aborted {
                        "Grok cancelled compaction"
                    } else {
                        "Grok reported a compaction error"
                    })
                    .into(),
            }
        }
    };
    if manual.is_some() {
        context["compaction"] = Value::Null;
    }
    Some(observation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_live_native_evidence_can_complete_a_compaction() {
        let mut context =
            json!({"native":"session","compaction":{"operation":"manual","request_id":1}});
        let mut frame = json!({"method":"_x.ai/session_notification","params":{
            "sessionId":"another","_meta":{"eventId":"42"},"update":{"sessionUpdate":"auto_compact_completed","tokens_before":100,"tokens_after":100}
        }});
        assert!(observe(&mut context, &frame).is_none());
        frame["params"]["sessionId"] = json!("session");
        frame["params"]["_meta"]["isReplay"] = json!(true);
        assert!(observe(&mut context, &frame).is_none());
        assert!(matches!(
            observe(&mut context, &json!({"id":1,"result":{}})),
            Some(Observation::CompactionFailed {
                manual: true,
                aborted: false,
                ..
            })
        ));
        frame["params"]["_meta"]["isReplay"] = json!(false);
        assert!(matches!(observe(&mut context, &frame),
            Some(Observation::Compacted { manual:false, operation, tokens_before:Some(100), tokens_after:Some(100), .. }) if operation == "grok:42"));
    }
}
