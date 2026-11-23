use std::sync::Arc;

use pontia_agent_control::{
    client_contract::{
        ClientData, NativeEventEvidence, TimelineBoundaryBackend, TurnTimelineBackend,
        TurnTopologyBackend,
        history::{RecoveredTurnHistory, TurnHistoryCandidate},
        native_history::{HistoryRead, NativeHistory},
        raw_transcripts::*,
    },
    clients::BranchTargetRequest,
};
use pontia_core::{
    Error, Result,
    domain::{DomainEvent, EventType},
};
use serde_json::{Value, json};

use crate::{
    adapter::AcpAdapter,
    peer::{Peers, SessionState},
};

#[derive(Clone, Default)]
pub(crate) struct Message {
    pub role: &'static str,
    pub text: String,
    pub display: Option<String>,
    pub prompt: Option<usize>,
    pub interjection: bool,
    pub(crate) tool: Option<(String, Option<String>)>,
}

pub(crate) fn user_bytes(messages: &[Message]) -> usize {
    messages
        .iter()
        .filter(|message| message.role == "user")
        .map(|message| message.text.len())
        .sum()
}

pub(crate) fn append(messages: &mut Vec<Message>, role: &'static str, text: &str) {
    if let Some(last) = messages.last_mut().filter(|last| last.role == role) {
        last.text.push_str(text);
    } else {
        messages.push(Message {
            role,
            text: text.into(),
            ..Default::default()
        });
    }
}

pub(crate) fn unavailable() -> Error {
    Error::CapabilityUnavailable(
        "source_unavailable: ACP history requires a current successful session/load".into(),
    )
}

// Cursors measure UTF-8 bytes in the native user-text stream, not chunks or
// guessed rounds. Rechunking and prompts with no Agent output preserve offsets.
pub(crate) fn collect(
    messages: &mut Vec<Message>,
    update: &Value,
    protocol: Option<&crate::InterjectionProtocol>,
) -> Result<()> {
    if protocol.is_some()
        && update["sessionUpdate"] == "tool_call"
        && let (Some(call), Some(title)) = (update["toolCallId"].as_str(), update["title"].as_str())
    {
        messages.push(Message {
            role: "tool_call",
            text: title.into(),
            tool: Some((call.into(), update["status"].as_str().map(str::to_owned))),
            ..Default::default()
        });
        return Ok(());
    }
    let role = match update["sessionUpdate"].as_str() {
        Some("user_message_chunk") => "user",
        Some("agent_message_chunk") => "assistant",
        _ => return Ok(()),
    };
    let text = update
        .pointer("/content/text")
        .and_then(Value::as_str)
        .ok_or_else(unavailable)?;
    if role == "assistant" && messages.is_empty() {
        return Err(unavailable());
    }
    if let Some(protocol) = protocol.filter(|_| role == "user") {
        let prompt = update
            .pointer(protocol.prompt_index)
            .and_then(Value::as_u64)
            .map(|n| n as usize);
        let interjection = update.pointer(protocol.interjection_marker) == Some(&Value::Bool(true));
        if interjection || prompt.is_some() {
            messages.push(Message {
                role,
                text: text.into(),
                prompt,
                interjection,
                tool: None,
                display: interjection
                    .then(|| {
                        update
                            .pointer(protocol.display_text)?
                            .as_str()
                            .map(str::to_owned)
                    })
                    .flatten(),
            });
            return Ok(());
        }
    }
    append(messages, role, text);
    Ok(())
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub(crate) enum Anchor {
    Bytes(usize),
    Prompt { prompt: usize },
}

impl Anchor {
    fn offset(self, messages: &[Message]) -> Result<usize> {
        match self {
            Self::Bytes(n) => Ok(n),
            Self::Prompt { prompt } => {
                let mut offset = 0;
                let mut next = 0;
                for message in messages.iter().filter(|m| m.role == "user") {
                    if let Some(index) = message.prompt {
                        if index == prompt {
                            return Ok(offset);
                        }
                        next = index + 1;
                    }
                    offset += message.text.len();
                }
                (next == prompt).then_some(offset).ok_or_else(unavailable)
            }
        }
    }
}

fn cursor(binding: &AgentBindingResolveRequest, index: Anchor) -> String {
    json!([
        "acp-replay-v1",
        binding.id,
        binding.client_session_key,
        index
    ])
    .to_string()
}
fn index(binding: &AgentBindingResolveRequest, value: &str) -> Result<Anchor> {
    let parsed: Value = serde_json::from_str(value)?;
    let index: Anchor = serde_json::from_value(parsed[3].clone()).map_err(|_| unavailable())?;
    if cursor(binding, index) != value {
        return Err(unavailable());
    }
    Ok(index)
}

impl ClientData for AcpAdapter {
    fn normalize_payload(&self, _kind: EventType, data: Value) -> Result<Value> {
        Ok(data)
    }
    fn take_evidence(&self, event: &mut DomainEvent) -> NativeEventEvidence {
        let anchor = event
            .payload
            .as_object_mut()
            .and_then(|p| p.remove("acp_history_anchor"));
        NativeEventEvidence {
            entry_anchor: anchor.map(|value| value.to_string()),
            topology: None,
        }
    }
    fn probe_timeline(&self, binding: &AgentBindingResolveRequest) -> Option<Result<()>> {
        // Contention is not evidence of source failure. The application has
        // already awaited our dynamic capabilities; keep that observation when
        // a synchronous probe cannot obtain a newer one without blocking.
        let peers = self.peers.try_lock().ok()?;
        let Some(peer) = peers.get(&binding.session_id) else {
            return Some(Err(unavailable()));
        };
        let state = peer.state.try_lock().ok()?;
        Some((|| {
            let identity = crate::identity::decode(&binding.client_session_key)?;
            if !state.current
                || !state.ready
                || state.native.as_ref().map(ToString::to_string).as_deref() != Some(&identity[1])
            {
                return Err(unavailable());
            }
            if let Some(reason) = &state.history_error {
                return Err(Error::CapabilityUnavailable(reason.clone()));
            }
            if state.replay.is_some() {
                return Err(Error::Conflict {
                    code: "timeline_pending",
                    message: "ACP replay in progress".into(),
                });
            }
            Ok(())
        })())
    }
    fn native_history(
        &self,
        binding: AgentBindingResolveRequest,
        _control: Option<pontia_agent_control::clients::ClientControlService>,
    ) -> Option<Arc<dyn NativeHistory>> {
        Some(Arc::new(AcpHistory {
            peers: self.peers.clone(),
            binding,
        }))
    }
    fn capture_native_boundary(
        &self,
        binding: &AgentBindingResolveRequest,
        kind: TimelineBoundaryCaptureKind,
        anchor: Option<String>,
        _first: bool,
        head: Option<&str>,
    ) -> Option<Result<CapturedTimelineBoundary>> {
        Some((|| {
            if let Some(head) = head {
                index(binding, head)?;
            }
            let position = anchor
                .ok_or_else(unavailable)?
                .parse::<Value>()
                .map_err(|_| unavailable())?;
            Ok(CapturedTimelineBoundary {
                kind,
                cursor: cursor(binding, serde_json::from_value(position)?),
            })
        })())
    }
    fn timeline(&self) -> TurnTimelineBackend {
        TurnTimelineBackend {
            resolver: Box::new(ProtocolOnly),
            reader: Box::new(ProtocolOnly),
        }
    }
    fn boundaries(&self) -> TimelineBoundaryBackend {
        TimelineBoundaryBackend {
            resolver: Box::new(ProtocolOnly),
            capturer: Box::new(ProtocolOnly),
        }
    }
    fn topology(&self) -> Option<TurnTopologyBackend> {
        None
    }
    fn branch_target(&self, _request: BranchTargetRequest) -> Result<String> {
        Err(unavailable())
    }
}

struct AcpHistory {
    peers: Peers,
    binding: AgentBindingResolveRequest,
}
impl AcpHistory {
    async fn snapshot(&self) -> Result<tokio::sync::OwnedMutexGuard<SessionState>> {
        let peer = self
            .peers
            .lock()
            .await
            .get(&self.binding.session_id)
            .cloned()
            .ok_or_else(unavailable)?;
        let state = peer.state.lock().await;
        let identity = crate::identity::decode(&self.binding.client_session_key)?;
        if !state.current
            || !state.ready
            || state.native.as_ref().map(ToString::to_string).as_deref() != Some(&identity[1])
        {
            return Err(unavailable());
        }
        drop(state);
        peer.load().await?;
        let state = peer.state.clone().lock_owned().await;
        if !state.current || state.history_error.is_some() || state.replay.is_some() {
            return Err(unavailable());
        }
        if state.steering_history_pending || !state.interjections.is_empty() {
            return Err(Error::Conflict {
                code: "timeline_pending",
                message: "Waiting for native interjection history".into(),
            });
        }
        Ok(state)
    }

    fn offsets(
        &self,
        turn_id: &str,
        head: &str,
        tail: Option<&str>,
        messages: &[Message],
    ) -> std::result::Result<(usize, usize), TurnTimelineReadError> {
        let total = user_bytes(messages);
        let start = index(&self.binding, head)?.offset(messages)?;
        let end = tail
            .map(|tail| index(&self.binding, tail)?.offset(messages))
            .transpose()?
            .unwrap_or(total);
        if start > end || end > total {
            return Err(TurnTimelineReadError::InvalidRange {
                turn_id: turn_id.into(),
                message: "ACP replay no longer contains this range".into(),
            });
        }
        Ok((start, end))
    }
}
impl NativeHistory for AcpHistory {
    fn read_session_timeline(
        &self,
        selection: SessionTimelineSelection,
    ) -> HistoryRead<'_, std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError>> {
        Box::pin(async move {
            let state = self.snapshot().await?;
            let mut items = Vec::new();
            for (number, turn) in selection.turns.iter().enumerate() {
                if !selection.selected_turn_ids.contains(&turn.turn_id) {
                    continue;
                }
                let head = turn.head_cursor.as_deref().ok_or_else(unavailable)?;
                let (start, end) = self.offsets(
                    &turn.turn_id,
                    head,
                    turn.tail_cursor.as_deref(),
                    &state.history,
                )?;
                items.extend(render(&state.history, start, end, Some(&turn.turn_id))?);
                // A checkpoint between two Turns belongs on the following page.
                let last = number + 1 == selection.turns.len();
                let previous_end = selection.turns[..number]
                    .iter()
                    .rev()
                    .find_map(|t| t.tail_cursor.as_deref())
                    .map(|tail| index(&self.binding, tail)?.offset(&state.history))
                    .transpose()?
                    .unwrap_or(0);
                items.extend(
                    state
                        .checkpoints
                        .iter()
                        .filter(|cp| cp.offset >= previous_end && (last || cp.offset < end))
                        .map(|cp| cp.item()),
                );
            }
            if selection.turns.is_empty() {
                items.extend(state.checkpoints.iter().map(|cp| cp.item()));
            }
            items.sort_by(|a, b| a.item.item_id.cmp(&b.item.item_id));
            Ok(items)
        })
    }
    fn read_ranges(
        &self,
        ranges: Vec<TurnTimelineRange>,
    ) -> HistoryRead<'_, std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError>> {
        Box::pin(async move {
            let state = self.snapshot().await?;
            let mut items = Vec::new();
            if ranges.is_empty() {
                return render(&state.history, 0, user_bytes(&state.history), None)
                    .map_err(Into::into);
            }
            for range in ranges {
                let (start, end) = self.offsets(
                    &range.turn_id,
                    &range.head_cursor,
                    range.tail_cursor.as_deref(),
                    &state.history,
                )?;
                items.extend(render(&state.history, start, end, Some(&range.turn_id))?);
            }
            Ok(items)
        })
    }
    fn recover(
        &self,
        _turns: Vec<TurnHistoryCandidate>,
    ) -> HistoryRead<'_, Result<Vec<RecoveredTurnHistory>>> {
        // Replay has no native Turn IDs. Never guess associations from summaries.
        Box::pin(async { Ok(Vec::new()) })
    }
    fn branch_target(&self, _range: TurnTimelineRange) -> HistoryRead<'_, Result<String>> {
        Box::pin(async { Err(unavailable()) })
    }
}

fn render(
    messages: &[Message],
    start: usize,
    end: usize,
    turn: Option<&str>,
) -> Result<Vec<TurnTimelineItem>> {
    let mut position = 0;
    let mut items = Vec::new();
    let has_tools = messages.iter().any(|m| m.tool.is_some());
    for (order, message) in messages.iter().enumerate() {
        let offset = position;
        let text = if message.role == "user" {
            position += message.text.len();
            let from = start.max(offset).min(position) - offset;
            let to = end.min(position).max(offset) - offset;
            if from >= to {
                continue;
            }
            if from == 0 && to == message.text.len() {
                message.display.as_deref().unwrap_or(&message.text)
            } else {
                message.text.get(from..to).ok_or_else(unavailable)?
            }
        } else {
            if position <= start || position > end {
                continue;
            }
            &message.text
        };
        if text.is_empty() {
            continue;
        }
        items.push(TurnTimelineItem {
            turn_id: turn.map(str::to_owned),
            item: TimelineItem {
                // Timeline consumers use item IDs as ordering keys when ACP
                // provides no timestamp. Preserve numeric native order lexically.
                item_id: if has_tools && message.role != "user" {
                    format!("acp:{:020}:assistant:{order:020}", start.max(offset))
                } else {
                    format!("acp:{:020}:{}", start.max(offset), message.role)
                },
                kind: message.role.into(),
                raw_kind: None,
                role: message.role.into(),
                title: message.tool.as_ref().map(|_| message.text.clone()),
                status: message.tool.as_ref().and_then(|(_, status)| status.clone()),
                occurred_at: None,
                content_preview: text.into(),
                call_id: message.tool.as_ref().map(|(id, _)| id.clone()),
                managed_tool_use: None,
            },
        });
    }
    Ok(items)
}

// ACP has no file source. The application uses the asynchronous NativeHistory
// and identity-based boundary hooks above; file-only entry points must fail.
struct ProtocolOnly;
impl AgentBindingResolver for ProtocolOnly {
    fn client_type(&self) -> &'static str {
        "acp"
    }
    fn resolve(&self, _request: &AgentBindingResolveRequest) -> Result<ResolvedAgentBinding> {
        Err(unavailable())
    }
}
impl TimelineBoundaryCapturer for ProtocolOnly {
    fn client_type(&self) -> &'static str {
        "acp"
    }
    fn capture_boundary(
        &self,
        _request: TimelineBoundaryCaptureRequest,
    ) -> Result<CapturedTimelineBoundary> {
        Err(unavailable())
    }
    fn capture_source_origin_head(
        &self,
        _id: &str,
        _anchor: Option<String>,
    ) -> Result<CapturedTimelineBoundary> {
        Err(unavailable())
    }
}
impl TurnTimelineReader for ProtocolOnly {
    fn client_type(&self) -> &'static str {
        "acp"
    }
    fn read_turn_ranges(
        &self,
        _request: TurnTimelineReadRequest,
    ) -> std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError> {
        Err(unavailable().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_interjections_render_in_native_order_and_in_the_consuming_prompt() {
        let agents = crate::AcpAgents::builtin();
        let recipe = agents.get("grok").unwrap().interjection;
        for line in include_str!("../tests/fixtures/interjection-history.jsonl").lines() {
            let case: Value = serde_json::from_str(line).unwrap();
            let mut messages = Vec::new();
            for update in case["updates"].as_array().unwrap() {
                collect(&mut messages, update, recipe).unwrap();
            }
            let promoted = matches!(case["case"].as_str(), Some("E8" | "E9" | "E10" | "E12"));
            let mut expected_user_texts =
                vec![case["updates"][0]["content"]["text"].as_str().unwrap()];
            let sent = case["sent"].as_array().unwrap();
            if !promoted {
                expected_user_texts.extend(sent.iter().map(|v| v.as_str().unwrap()));
            }
            let end = Anchor::Prompt { prompt: 1 }.offset(&messages).unwrap();
            let first = render(&messages, 0, end, Some("original")).unwrap();
            assert_eq!(
                first
                    .iter()
                    .filter(|i| i.item.role == "user")
                    .map(|i| i.item.content_preview.as_str())
                    .collect::<Vec<_>>(),
                expected_user_texts
            );
            assert!(
                first
                    .windows(2)
                    .all(|pair| pair[0].item.item_id < pair[1].item.item_id)
            );
            if case["case"] == "E5" {
                let i = first
                    .iter()
                    .position(|i| i.item.content_preview == sent[0])
                    .unwrap();
                assert_eq!(first[i - 1].item.kind, "tool_call");
                assert_eq!(first[i + 1].item.role, "assistant");
            }
            if promoted {
                for (i, text) in sent.iter().enumerate() {
                    let start = Anchor::Prompt { prompt: i + 1 }.offset(&messages).unwrap();
                    let end = Anchor::Prompt { prompt: i + 2 }.offset(&messages).unwrap();
                    let followup = render(&messages, start, end, Some("promotion")).unwrap();
                    assert_eq!(followup[0].item.content_preview, text.as_str().unwrap());
                    assert_eq!(followup[1].item.role, "assistant");
                }
            }
        }
    }
}
