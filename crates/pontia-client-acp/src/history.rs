use std::sync::Arc;

use pontia_application::{
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
    peer::{Peer, Peers},
};

#[derive(Clone)]
pub(crate) struct Message {
    pub role: &'static str,
    pub text: String,
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
pub(crate) fn collect(messages: &mut Vec<Message>, update: &Value) -> Result<()> {
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
    append(messages, role, text);
    Ok(())
}

fn cursor(binding: &AgentBindingResolveRequest, index: usize) -> String {
    json!([
        "acp-replay-v1",
        binding.id,
        binding.client_session_key,
        index
    ])
    .to_string()
}
fn index(binding: &AgentBindingResolveRequest, value: &str) -> Result<usize> {
    let parsed: Value = serde_json::from_str(value)?;
    let index = parsed
        .get(3)
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(unavailable)?;
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
            entry_anchor: anchor.and_then(|v| v.as_u64().map(|n| n.to_string())),
            topology: None,
        }
    }
    fn probe_timeline(&self, binding: &AgentBindingResolveRequest) -> Option<Result<()>> {
        Some((|| {
            let peers = self.peers.try_lock().map_err(|_| unavailable())?;
            let peer = peers.get(&binding.session_id).ok_or_else(unavailable)?;
            let state = peer.state.try_lock().map_err(|_| unavailable())?;
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
        _control: Option<pontia_application::clients::ClientControlService>,
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
                .parse::<usize>()
                .map_err(|_| unavailable())?;
            Ok(CapturedTimelineBoundary {
                kind,
                cursor: cursor(binding, position),
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
    async fn peer(&self) -> Result<Arc<Peer>> {
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
        Ok(peer)
    }
}
impl NativeHistory for AcpHistory {
    fn read_ranges(
        &self,
        ranges: Vec<TurnTimelineRange>,
    ) -> HistoryRead<'_, std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError>> {
        Box::pin(async move {
            let peer = self.peer().await?;
            peer.load().await?;
            let state = peer.state.lock().await;
            if !state.current || state.history_error.is_some() || state.replay.is_some() {
                return Err(unavailable().into());
            }
            let mut items = Vec::new();
            if ranges.is_empty() {
                return render(&state.history, 0, user_bytes(&state.history), None)
                    .map_err(Into::into);
            }
            for range in ranges {
                let start = index(&self.binding, &range.head_cursor)?;
                let end = range
                    .tail_cursor
                    .as_deref()
                    .map(|tail| index(&self.binding, tail))
                    .transpose()?
                    .unwrap_or_else(|| user_bytes(&state.history));
                if start > end || end > user_bytes(&state.history) {
                    return Err(TurnTimelineReadError::InvalidRange {
                        turn_id: range.turn_id,
                        message: "ACP replay no longer contains this range".into(),
                    });
                }
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
    for message in messages {
        let offset = position;
        let text = if message.role == "user" {
            position += message.text.len();
            let from = start.max(offset).min(position) - offset;
            let to = end.min(position).max(offset) - offset;
            if from >= to {
                continue;
            }
            message.text.get(from..to).ok_or_else(unavailable)?
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
                item_id: format!("acp:{:020}:{}", start.max(offset), message.role),
                kind: message.role.into(),
                raw_kind: None,
                role: message.role.into(),
                title: None,
                status: None,
                occurred_at: None,
                content_preview: text.into(),
                call_id: None,
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
