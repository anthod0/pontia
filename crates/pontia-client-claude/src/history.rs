use crate::{Adapter, unavailable};
use pontia_application::{
    AgentBindingService,
    client_contract::{
        history::{RecoveredTurnHistory, TurnHistoryCandidate},
        native_history::{HistoryRead, NativeHistory},
        raw_transcripts::*,
        *,
    },
};
use pontia_core::{
    Error, Result,
    domain::{DomainEvent, EventType, TurnState},
};
use serde_json::{Value, json};
use std::{path::Path, process::Stdio, sync::Arc, time::Duration};
use tokio::{io::AsyncWriteExt, process::Command, sync::OnceCell};

fn source_unavailable() -> Error {
    Error::CapabilityUnavailable(
        "source_unavailable: Claude Agent native history is missing or unreadable".into(),
    )
}

pub(crate) async fn read_sdk(
    script: &Path,
    session: &str,
    cwd: &str,
    config: &Path,
) -> Result<Vec<Value>> {
    let operation = async {
        let mut child = Command::new(crate::setup::node())
            .arg(script)
            .arg("history")
            .env("CLAUDE_CONFIG_DIR", config)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| source_unavailable())?;
        let mut stdin = child.stdin.take().ok_or_else(source_unavailable)?;
        stdin
            .write_all(format!("{}\n", json!({"session_id":session,"cwd":cwd})).as_bytes())
            .await
            .map_err(|_| source_unavailable())?;
        drop(stdin);
        let output = child
            .wait_with_output()
            .await
            .map_err(|_| source_unavailable())?;
        if !output.status.success() {
            return Err(source_unavailable());
        }
        let messages: Vec<Value> =
            serde_json::from_slice(&output.stdout).map_err(|_| source_unavailable())?;
        if messages.is_empty() {
            return Err(source_unavailable());
        }
        if messages
            .iter()
            .any(|message| message["session_id"] != session)
        {
            return Err(Error::Conflict {
                code: "timeline_source_identity_mismatch",
                message: "Claude Agent history identity mismatch".into(),
            });
        }
        Ok(messages)
    };
    tokio::time::timeout(Duration::from_secs(30), operation)
        .await
        .map_err(|_| source_unavailable())?
}

fn cursor(binding: &AgentBindingResolveRequest, anchor: &str) -> String {
    json!([
        "claude-message-v1",
        binding.id,
        binding.client_session_key,
        anchor
    ])
    .to_string()
}
fn anchor(binding: &AgentBindingResolveRequest, value: &str) -> Result<String> {
    let parsed: Value = serde_json::from_str(value)?;
    let id = parsed[3]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or_else(source_unavailable)?;
    if cursor(binding, id) != value {
        return Err(source_unavailable());
    }
    Ok(id.into())
}

impl ClientData for Adapter {
    fn normalize_payload(&self, _: EventType, data: Value) -> Result<Value> {
        Ok(data)
    }
    fn take_evidence(&self, event: &mut DomainEvent) -> NativeEventEvidence {
        NativeEventEvidence {
            entry_anchor: event
                .payload
                .as_object_mut()
                .and_then(|p| p.remove("claude_history_anchor"))
                .and_then(|v| v.as_str().map(str::to_owned)),
            topology: None,
        }
    }
    fn native_history(
        &self,
        binding: AgentBindingResolveRequest,
        _: Option<pontia_application::clients::ClientControlService>,
    ) -> Option<Arc<dyn NativeHistory>> {
        Some(Arc::new(History {
            adapter: self.clone(),
            binding,
            messages: OnceCell::new(),
        }))
    }
    fn capture_native_boundary(
        &self,
        binding: &AgentBindingResolveRequest,
        kind: TimelineBoundaryCaptureKind,
        entry: Option<String>,
        _: bool,
        head: Option<&str>,
    ) -> Option<Result<CapturedTimelineBoundary>> {
        Some((|| {
            if let Some(head) = head {
                anchor(binding, head)?;
            }
            let entry = entry
                .filter(|id| !id.is_empty())
                .ok_or_else(source_unavailable)?;
            Ok(CapturedTimelineBoundary {
                kind,
                cursor: cursor(binding, &entry),
            })
        })())
    }
    fn timeline(&self) -> TurnTimelineBackend {
        TurnTimelineBackend {
            resolver: Box::new(SdkOnly),
            reader: Box::new(SdkOnly),
        }
    }
    fn boundaries(&self) -> TimelineBoundaryBackend {
        TimelineBoundaryBackend {
            resolver: Box::new(SdkOnly),
            capturer: Box::new(SdkOnly),
        }
    }
    fn topology(&self) -> Option<TurnTopologyBackend> {
        None
    }
    fn branch_target(&self, _: BranchTargetRequest) -> Result<String> {
        Err(crate::unsupported())
    }
}

struct History {
    adapter: Adapter,
    binding: AgentBindingResolveRequest,
    messages: OnceCell<Vec<Value>>,
}
impl History {
    async fn messages(&self) -> Result<&Vec<Value>> {
        self.messages
            .get_or_try_init(|| async {
                let (pool, root) = self.adapter.context.get().ok_or_else(unavailable)?;
                let binding = AgentBindingService::new(pool.clone())
                    .binding_for_session(&self.binding.session_id)
                    .await?
                    .ok_or_else(source_unavailable)?;
                if binding.id != self.binding.id
                    || binding.client_session_key != self.binding.client_session_key
                {
                    return Err(source_unavailable());
                }
                let config = binding.metadata["claude_config_dir"]
                    .as_str()
                    .ok_or_else(source_unavailable)?;
                read_sdk(
                    &self.adapter.script(root)?,
                    &binding.client_session_key,
                    &binding.launch_cwd,
                    Path::new(config),
                )
                .await
            })
            .await
    }
    fn position(
        &self,
        messages: &[Value],
        value: &str,
        turn: &str,
    ) -> std::result::Result<usize, TurnTimelineReadError> {
        let id = anchor(&self.binding, value)?;
        messages
            .iter()
            .position(|message| message["uuid"] == id)
            .ok_or_else(|| TurnTimelineReadError::InvalidRange {
                turn_id: turn.into(),
                message: "Native history no longer contains the recorded message boundary".into(),
            })
    }
}
impl NativeHistory for History {
    fn read_ranges(
        &self,
        ranges: Vec<TurnTimelineRange>,
    ) -> HistoryRead<'_, std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError>> {
        Box::pin(async move {
            let messages = self.messages().await?;
            let mut items = Vec::new();
            // A persisted input can precede the first authoritative Turn start.
            // The Turn timeline cannot claim that such a conversation is empty.
            if ranges.is_empty() {
                return Err(Error::CapabilityUnavailable(
                    "Claude Agent history has no confirmed Turn association".into(),
                )
                .into());
            }
            for range in ranges {
                let start = self.position(messages, &range.head_cursor, &range.turn_id)?;
                let end = match &range.tail_cursor {
                    Some(tail) => self.position(messages, tail, &range.turn_id)? + 1,
                    None => messages.len(),
                };
                if start >= end {
                    return Err(TurnTimelineReadError::InvalidRange {
                        turn_id: range.turn_id,
                        message: "Reversed native message boundaries".into(),
                    });
                }
                for (index, message) in messages.iter().enumerate().take(end).skip(start) {
                    render(message, index, &range.turn_id, &mut items);
                }
            }
            Ok(items)
        })
    }
    fn recover(
        &self,
        turns: Vec<TurnHistoryCandidate>,
    ) -> HistoryRead<'_, Result<Vec<RecoveredTurnHistory>>> {
        Box::pin(async move {
            let messages = self.messages().await?;
            let (pool, _) = self.adapter.context.get().ok_or_else(unavailable)?;
            let mut recovered = Vec::new();
            let closes: Vec<String> = sqlx::query_scalar("SELECT payload FROM events WHERE session_id=? AND event_type='runtime.exited' ORDER BY rowid")
                .bind(&self.binding.session_id).fetch_all(pool).await?;
            let mut closed_anchors = std::collections::HashMap::new();
            for payload in closes {
                let payload: Value = serde_json::from_str(&payload)?;
                if let (Some(input), Some(anchor)) = (
                    payload["claude_closed_input"].as_str(),
                    payload["claude_closed_history_anchor"].as_str(),
                ) {
                    closed_anchors.insert(input.to_owned(), anchor.to_owned());
                }
            }
            // Input UUIDs are durable execution evidence, never matched by text.
            let rows: Vec<(String, String)> = sqlx::query_as("SELECT turn_id, payload FROM events WHERE session_id=? AND event_type='turn.started' AND turn_id IS NOT NULL ORDER BY rowid")
                .bind(&self.binding.session_id).fetch_all(pool).await?;
            let mut positions = Vec::new();
            for (turn, payload) in rows {
                let payload: Value = serde_json::from_str(&payload)?;
                if let Some(id) = payload["claude_input_id"].as_str()
                    && let Some(position) = messages.iter().position(|m| m["uuid"] == id)
                {
                    positions.push((turn, position));
                }
            }
            positions.sort_by_key(|(_, position)| *position);
            for (index, (id, position)) in positions.iter().enumerate() {
                let Some(turn) = turns.iter().find(|t| t.turn_id == *id) else {
                    continue;
                };
                let tail = if turn.state == TurnState::Abandoned && turn.tail_cursor.is_none() {
                    messages[*position]["uuid"]
                        .as_str()
                        .and_then(|input| closed_anchors.get(input))
                        .filter(|anchor| {
                            messages
                                .iter()
                                .any(|m| m["uuid"].as_str() == Some(anchor.as_str()))
                        })
                        .map(|id| cursor(&self.binding, id))
                } else {
                    None
                };
                recovered.push(RecoveredTurnHistory {
                    turn_id: id.clone(),
                    head_cursor: turn.head_cursor.is_none().then(|| {
                        cursor(
                            &self.binding,
                            messages[*position]["uuid"].as_str().expect("matched UUID"),
                        )
                    }),
                    tail_cursor: tail,
                    topology: TopologyResolveResult {
                        resolution: if index == 0 && *position == 0 {
                            TopologyResolution::Root
                        } else if index > 0 {
                            TopologyResolution::Linked {
                                parent_turn_id: positions[index - 1].0.clone(),
                            }
                        } else {
                            TopologyResolution::Unknown
                        },
                        diagnostic: TopologyDiagnostic::ParentMatched,
                    },
                });
            }
            Ok(recovered)
        })
    }
    fn branch_target(&self, _: TurnTimelineRange) -> HistoryRead<'_, Result<String>> {
        Box::pin(async { Err(crate::unsupported()) })
    }
}

fn render(message: &Value, position: usize, turn: &str, items: &mut Vec<TurnTimelineItem>) {
    let role = message["type"].as_str().unwrap_or("unknown");
    let content = &message["message"]["content"];
    let blocks = content
        .as_array()
        .cloned()
        .unwrap_or_else(|| vec![json!({"type":"text","text":content.as_str().unwrap_or("")})]);
    for (index, block) in blocks.iter().enumerate() {
        let (kind, role, text, title, call_id, status) = match block["type"].as_str() {
            Some("text") => (
                role,
                role,
                block["text"].as_str().unwrap_or("").into(),
                None,
                None,
                None,
            ),
            Some("thinking") => (
                "thinking",
                "assistant",
                block["thinking"].as_str().unwrap_or("").into(),
                None,
                None,
                None,
            ),
            Some("tool_use") => (
                "tool_call",
                "assistant",
                block["input"].to_string(),
                block["name"].as_str().map(str::to_owned),
                block["id"].as_str().map(str::to_owned),
                None,
            ),
            Some("tool_result") => (
                "tool_result",
                "tool",
                block["content"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| block["content"].to_string()),
                None,
                block["tool_use_id"].as_str().map(str::to_owned),
                Some(
                    if block["is_error"] == true {
                        "failed"
                    } else {
                        "completed"
                    }
                    .into(),
                ),
            ),
            _ => continue,
        };
        items.push(TurnTimelineItem {
            turn_id: Some(turn.into()),
            item: TimelineItem {
                item_id: format!(
                    "claude:{position:020}:{}:{index:05}",
                    message["uuid"].as_str().unwrap_or("")
                ),
                kind: kind.into(),
                role: role.into(),
                raw_kind: block["type"].as_str().map(str::to_owned),
                title,
                status,
                occurred_at: None,
                content_preview: text,
                call_id,
                managed_tool_use: None,
            },
        });
    }
}

// All reads go through the SDK helper, including while the execution host is offline.
struct SdkOnly;
impl AgentBindingResolver for SdkOnly {
    fn client_type(&self) -> &'static str {
        "claude-agent"
    }
    fn resolve(&self, _: &AgentBindingResolveRequest) -> Result<ResolvedAgentBinding> {
        Err(source_unavailable())
    }
}
impl TimelineBoundaryCapturer for SdkOnly {
    fn client_type(&self) -> &'static str {
        "claude-agent"
    }
    fn capture_boundary(
        &self,
        _: TimelineBoundaryCaptureRequest,
    ) -> Result<CapturedTimelineBoundary> {
        Err(source_unavailable())
    }
    fn capture_source_origin_head(
        &self,
        _: &str,
        _: Option<String>,
    ) -> Result<CapturedTimelineBoundary> {
        Err(source_unavailable())
    }
}
impl TurnTimelineReader for SdkOnly {
    fn client_type(&self) -> &'static str {
        "claude-agent"
    }
    fn read_turn_ranges(
        &self,
        _: TurnTimelineReadRequest,
    ) -> std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError> {
        Err(source_unavailable().into())
    }
}
