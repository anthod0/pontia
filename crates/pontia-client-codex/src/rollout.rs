use pontia_agent_control::client_contract::raw_transcripts::{
    AgentBindingResolveRequest, AgentBindingResolver, CapturedTimelineBoundary,
    ResolvedAgentBinding, TimelineBoundaryCaptureKind, TimelineBoundaryCaptureRequest,
    TimelineBoundaryCapturer, TimelineItem, TurnTimelineItem, TurnTimelineReadError,
    TurnTimelineReadRequest, TurnTimelineReader,
};
use pontia_core::{Error, Result};
use serde_json::Value;
use std::{collections::HashMap, path::Path};

pub struct CodexRollout;

pub fn identity(path: &Path) -> Result<String> {
    crate::history::files::header(path)?["payload"]["id"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::Domain("Codex rollout has no native identity".into()))
}

fn boundary(
    source: &ResolvedAgentBinding,
    value: &str,
    kind: TimelineBoundaryCaptureKind,
) -> Result<String> {
    crate::history::cursor::decode(
        value,
        &pontia_agent_control::client_contract::raw_transcripts::AgentBindingResolveRequest {
            id: source.id.clone(),
            session_id: String::new(),
            client_type: "codex".into(),
            client_session_key: source.fingerprint.clone().unwrap_or_default(),
            client_session_file: Some(source.path.clone()),
        },
        kind,
    )
}

impl AgentBindingResolver for CodexRollout {
    fn client_type(&self) -> &'static str {
        "codex"
    }
    fn resolve(&self, request: &AgentBindingResolveRequest) -> Result<ResolvedAgentBinding> {
        if request.client_type != "codex" {
            return Err(Error::CapabilityUnavailable(
                "Unsupported Codex binding client type".into(),
            ));
        }
        let path = request.client_session_file.clone().ok_or_else(|| {
            Error::CapabilityUnavailable("source_unavailable: Codex rollout path is missing".into())
        })?;
        let thread = identity(&path)?;
        if thread != request.client_session_key {
            return Err(Error::Conflict {
                code: "timeline_source_identity_mismatch",
                message: "Codex rollout does not match the bound thread".into(),
            });
        }
        Ok(ResolvedAgentBinding {
            id: request.id.clone(),
            client_type: "codex".into(),
            format: "codex_rollout".into(),
            path,
            fingerprint: Some(thread),
        })
    }
}

impl TimelineBoundaryCapturer for CodexRollout {
    fn client_type(&self) -> &'static str {
        "codex"
    }
    fn capture_boundary(
        &self,
        request: TimelineBoundaryCaptureRequest,
    ) -> Result<CapturedTimelineBoundary> {
        let turn = request
            .native_entry_anchor
            .ok_or_else(|| Error::Domain("Codex boundary requires native turn id".into()))?;
        let thread = request
            .source
            .fingerprint
            .as_deref()
            .ok_or_else(|| Error::Domain("Codex boundary requires thread identity".into()))?;
        Ok(CapturedTimelineBoundary {
            kind: request.kind,
            cursor: crate::history::cursor::encode(&request.source.id, thread, &turn, request.kind),
        })
    }
    fn capture_source_origin_head(
        &self,
        binding: &str,
        anchor: Option<String>,
    ) -> Result<CapturedTimelineBoundary> {
        let _ = (binding, anchor);
        Err(Error::Domain(
            "Codex boundary requires bound thread identity".into(),
        ))
    }
}

impl TurnTimelineReader for CodexRollout {
    fn client_type(&self) -> &'static str {
        "codex"
    }
    fn read_session_timeline(
        &self,
        request: pontia_agent_control::client_contract::raw_transcripts::SessionTimelineReadRequest,
    ) -> std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError> {
        use pontia_agent_control::client_contract::raw_transcripts::TurnTimelineRange;
        let selected: std::collections::HashSet<_> =
            request.selection.selected_turn_ids.iter().collect();
        let ranges = request
            .selection
            .turns
            .iter()
            .filter(|turn| selected.contains(&turn.turn_id))
            .filter_map(|turn| {
                turn.head_cursor.as_ref().map(|head| TurnTimelineRange {
                    turn_id: turn.turn_id.clone(),
                    is_first_session_turn: false,
                    head_cursor: head.clone(),
                    tail_cursor: turn.tail_cursor.clone(),
                })
            })
            .collect();
        let mut output = self.read_turn_ranges(TurnTimelineReadRequest {
            source: request.source.clone(),
            ranges,
        })?;
        let thread = identity(&request.source.path)?;
        let mut starts = Vec::new();
        let mut native_turns = HashMap::new();
        for turn in &request.selection.turns {
            let Some(head) = &turn.head_cursor else {
                continue;
            };
            let native = boundary(&request.source, head, TimelineBoundaryCaptureKind::Head)?;
            native_turns.insert(native, turn.turn_id.clone());
        }
        let records = crate::history::files::records(&request.source.path)?;
        for (offset, record) in &records {
            if let Some(turn) = native_turn(record).and_then(|id| native_turns.get(&id))
                && !starts.iter().any(|(_, existing)| existing == turn)
            {
                starts.push((*offset, turn.clone()));
            }
        }
        let mut current = None;
        let mut awaiting_confirmation = false;
        for (item_offset, record) in records {
            let event = record.pointer("/payload/type").and_then(Value::as_str);
            if record["type"] == "turn_context" || event == Some("task_started") {
                current = native_turn(&record);
            }
            let compacted = record["type"] == "compacted";
            let confirmed = record["type"] == "event_msg" && event == Some("context_compacted");
            if compacted || confirmed {
                if confirmed && awaiting_confirmation {
                    awaiting_confirmation = false;
                } else {
                    awaiting_confirmation = compacted;
                    let owner = native_turn(&record)
                        .or_else(|| current.clone())
                        .and_then(|native| native_turns.get(&native).cloned());
                    let page = owner.as_ref().or_else(|| {
                        starts
                            .iter()
                            .find(|(start, _)| *start > item_offset)
                            .or_else(|| starts.last())
                            .map(|(_, turn)| turn)
                    });
                    if request.selection.turns.is_empty()
                        || page.is_some_and(|turn| selected.contains(turn))
                    {
                        output.push(TurnTimelineItem {
                            turn_id: owner,
                            item: compaction_checkpoint(&thread, item_offset, &record),
                        });
                    }
                }
            }
            if matches!(event, Some("task_complete" | "turn_aborted")) {
                current = None;
            }
        }
        output.sort_by_key(|entry| {
            entry
                .item
                .item_id
                .rsplit(':')
                .next()
                .and_then(|offset| offset.parse::<u64>().ok())
                .unwrap_or_default()
        });
        Ok(output)
    }

    fn read_turn_ranges(
        &self,
        request: TurnTimelineReadRequest,
    ) -> std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError> {
        let thread = identity(&request.source.path)?;
        if request.source.fingerprint.as_deref() != Some(&thread) {
            return Err(Error::Conflict {
                code: "timeline_source_identity_mismatch",
                message: "Codex rollout identity changed while reading".into(),
            }
            .into());
        }
        let mut output = Vec::new();
        let records = crate::history::files::records(&request.source.path)?;
        for range in request.ranges {
            let invalid = |message: &str| TurnTimelineReadError::InvalidRange {
                turn_id: range.turn_id.clone(),
                message: message.into(),
            };
            let native = boundary(
                &request.source,
                &range.head_cursor,
                TimelineBoundaryCaptureKind::Head,
            )
            .map_err(|_| invalid("Invalid Codex head cursor"))?;
            if let Some(tail) = &range.tail_cursor {
                let tail = boundary(&request.source, tail, TimelineBoundaryCaptureKind::Tail)
                    .map_err(|_| invalid("Invalid Codex tail cursor"))?;
                if tail != native {
                    return Err(invalid("Codex cursor identity mismatch"));
                }
            }
            let mut current = None;
            let mut calls = HashMap::<String, Value>::new();
            for (item_offset, record) in &records {
                if (record["type"] == "turn_context"
                    || record.pointer("/payload/type").and_then(Value::as_str)
                        == Some("task_started"))
                    && let Some(turn) = native_turn(record)
                {
                    current = Some(turn);
                }
                let item_turn = native_turn(record).or_else(|| current.clone());
                if item_turn.as_deref() != Some(&native) {
                    continue;
                }
                if record["type"] == "compacted"
                    || record.pointer("/payload/type").and_then(Value::as_str)
                        == Some("context_compacted")
                {
                    continue; // Session reader owns checkpoint deduplication and gap ordering.
                }
                if record["type"] != "response_item" {
                    if let Some((kind, payload)) = native_detail(record) {
                        output.push(TurnTimelineItem {
                            turn_id: Some(range.turn_id.clone()),
                            item: TimelineItem {
                                item_id: format!("codex:{thread}:{item_offset}"),
                                kind: "tool_result".into(),
                                raw_kind: Some(kind.into()),
                                role: "tool".into(),
                                title: Some(kind.into()),
                                status: if kind == "error" {
                                    Some("error".into())
                                } else {
                                    payload["status"].as_str().map(str::to_string)
                                },
                                occurred_at: record["timestamp"].as_str().map(str::to_string),
                                content_preview: payload.to_string(),
                                call_id: None,
                                managed_tool_use: None,
                            },
                        });
                    }
                    continue;
                }
                let payload = &record["payload"];
                let raw_kind = payload["type"].as_str().unwrap_or("unknown");
                let role = payload["role"].as_str().unwrap_or("assistant");
                if raw_kind == "message" && !matches!(role, "user" | "assistant") {
                    continue;
                }
                if raw_kind == "message"
                    && role == "user"
                    && payload
                        .pointer("/internal_chat_message_metadata_passthrough/content_item_kinds")
                        .and_then(Value::as_array)
                        .is_some_and(|kinds| {
                            !kinds.is_empty()
                                && kinds.iter().all(|kind| {
                                    matches!(
                                        kind.as_str(),
                                        Some(
                                            "plugins.recommendations"
                                                | "agents_md.instructions"
                                                | "environments.environment_context"
                                        )
                                    )
                                })
                        })
                {
                    continue;
                }
                let mut title = None;
                let mut call_id = None;
                let (kind, role, content) = match raw_kind {
                    "message" => (role, role, render_content(&payload["content"])),
                    "reasoning" => ("thinking", "assistant", render_content(&payload["summary"])),
                    "function_call" | "custom_tool_call" => {
                        let name = payload["name"].as_str().unwrap_or("Tool");
                        title = Some(name.to_string());
                        if let Some(id) = payload["call_id"].as_str() {
                            call_id = Some(id.to_string());
                            calls.insert(id.into(), payload.clone());
                        }
                        (
                            "tool_call",
                            "assistant",
                            payload["arguments"]
                                .as_str()
                                .or_else(|| payload["input"].as_str())
                                .map(str::to_string)
                                .unwrap_or_else(|| payload.to_string()),
                        )
                    }
                    "function_call_output" | "custom_tool_call_output" => {
                        let result = payload["output"]
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| payload["output"].to_string());
                        call_id = payload["call_id"].as_str().map(str::to_string);
                        let call = call_id.as_deref().and_then(|id| calls.get(id));
                        let content = if let Some(call) =
                            call.filter(|call| call["name"] == "request_user_input")
                        {
                            title = Some("Questions and answers".into());
                            format!(
                                "Questions:\n{}\n\nAnswers:\n{}",
                                call["arguments"].as_str().unwrap_or(""),
                                result
                            )
                        } else {
                            title = call
                                .and_then(|call| call["name"].as_str())
                                .map(str::to_string);
                            result
                        };
                        ("tool_result", "tool", content)
                    }
                    _ => {
                        title = Some(raw_kind.into());
                        ("tool_result", "tool", payload.to_string())
                    }
                };
                output.push(TurnTimelineItem {
                    turn_id: Some(range.turn_id.clone()),
                    item: TimelineItem {
                        item_id: format!("codex:{thread}:{item_offset}"),
                        kind: kind.into(),
                        raw_kind: Some(raw_kind.into()),
                        role: role.into(),
                        title,
                        status: None,
                        occurred_at: record["timestamp"].as_str().map(str::to_string),
                        content_preview: content,
                        call_id,
                        managed_tool_use: None,
                    },
                });
            }
        }
        Ok(output)
    }
}

// Native execution details (including MCP, file changes and collaboration) can
// exist only in event records; retaining their structured payload avoids losing
// content that is absent from model-facing response items.
fn native_detail(record: &Value) -> Option<(&str, &Value)> {
    let payload = &record["payload"];
    match record["type"].as_str()? {
        "session_meta" | "turn_context" | "world_state" | "token_usage_record" => None,
        "event_msg" => match payload["type"].as_str()? {
            "task_complete" if !payload["error"].is_null() => Some(("error", &payload["error"])),
            "task_started"
            | "task_complete"
            | "turn_aborted"
            | "token_count"
            | "thread_settings_applied"
            | "user_message"
            | "agent_message"
            | "agent_reasoning"
            | "agent_reasoning_raw_content" => None,
            "item_started" if payload["item"]["type"] == "ContextCompaction" => None,
            "item_completed" => {
                let item = &payload["item"];
                match item["type"].as_str()? {
                    "UserMessage" | "AgentMessage" | "Reasoning" | "ContextCompaction" => None,
                    kind => Some((kind, item)),
                }
            }
            kind => Some((kind, payload)),
        },
        kind => Some((kind, payload)),
    }
}

fn native_turn(record: &Value) -> Option<String> {
    record
        .pointer("/payload/turn_id")
        .or_else(|| record.pointer("/payload/internal_chat_message_metadata_passthrough/turn_id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn render_content(value: &Value) -> String {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    item["text"]
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| item.to_string())
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| value.to_string())
}

fn compaction_checkpoint(thread: &str, offset: u64, record: &Value) -> TimelineItem {
    TimelineItem {
        item_id: format!("codex:{thread}:{offset}"),
        kind: "context_compaction".into(),
        raw_kind: Some("contextCompaction".into()),
        role: "system".into(),
        title: Some("Context compaction".into()),
        status: None,
        occurred_at: record["timestamp"].as_str().map(str::to_owned),
        content_preview: String::new(),
        call_id: None,
        managed_tool_use: None,
    }
}
