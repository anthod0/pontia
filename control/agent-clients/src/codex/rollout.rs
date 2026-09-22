use crate::raw_transcripts::*;
use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::Path,
};

pub struct CodexRollout;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    version: u8,
    binding: String,
    native_turn: String,
    offset: u64,
    thread: Option<String>,
}

pub fn identity(path: &Path) -> Result<String> {
    let mut reader = BufReader::new(
        File::open(path)
            .map_err(|e| Error::CapabilityUnavailable(format!("source_unavailable: {e}")))?,
    );
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let record: Value = serde_json::from_str(&line)?;
    if record["type"] != "session_meta"
        || record["payload"]["cli_version"] != super::SUPPORTED_VERSION
    {
        return Err(Error::CapabilityUnavailable(
            "Unsupported Codex rollout format/version".into(),
        ));
    }
    record["payload"]["id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| Error::Domain("Codex rollout has no native identity".into()))
}

impl AgentBindingResolver for CodexRollout {
    fn client_type(&self) -> &'static str {
        "codex"
    }
    fn resolve(&self, request: &AgentBindingResolveRequest) -> Result<ResolvedAgentBinding> {
        let path = request.client_session_file.clone().ok_or_else(|| {
            Error::CapabilityUnavailable("source_unavailable: Codex rollout path is missing".into())
        })?;
        let thread = identity(&path)?;
        Ok(ResolvedAgentBinding {
            id: request.id.clone(),
            client_type: "codex".into(),
            format: "codex_rollout_0.155.1".into(),
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
        let mut reader = BufReader::new(File::open(&request.source.path)?);
        let mut offset = 0;
        let mut head = None;
        let mut line = String::new();
        while reader.read_line(&mut line)? > 0 {
            if !line.ends_with('\n') {
                break;
            }
            let record: Value = serde_json::from_str(&line)?;
            if native_turn(&record).as_deref() == Some(&turn) && head.is_none() {
                head = Some(offset);
            }
            offset += line.len() as u64;
            line.clear();
        }
        let offset = match request.kind {
            TimelineBoundaryCaptureKind::Head => match head {
                Some(offset) => offset,
                None if request.allow_missing_native_entry_anchor => 0,
                None => {
                    return Err(Error::CapabilityUnavailable(
                        "Codex native turn anchor is not persisted yet".into(),
                    ));
                }
            },
            TimelineBoundaryCaptureKind::Tail => offset,
        };
        Ok(CapturedTimelineBoundary {
            kind: request.kind,
            cursor: serde_json::to_string(&Cursor {
                version: 1,
                binding: request.source.id,
                native_turn: turn,
                offset,
                thread: request.source.fingerprint,
            })?,
        })
    }
    fn capture_source_origin_head(
        &self,
        binding: &str,
        anchor: Option<String>,
    ) -> Result<CapturedTimelineBoundary> {
        let turn =
            anchor.ok_or_else(|| Error::Domain("Codex boundary requires native turn id".into()))?;
        Ok(CapturedTimelineBoundary {
            kind: TimelineBoundaryCaptureKind::Head,
            cursor: serde_json::to_string(&Cursor {
                version: 1,
                binding: binding.into(),
                native_turn: turn,
                offset: 0,
                thread: None,
            })?,
        })
    }
}

impl TurnTimelineReader for CodexRollout {
    fn client_type(&self) -> &'static str {
        "codex"
    }
    fn read_turn_ranges(
        &self,
        request: TurnTimelineReadRequest,
    ) -> std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError> {
        let thread = identity(&request.source.path)?;
        let mut output = Vec::new();
        for range in request.ranges {
            let invalid = |message: &str| TurnTimelineReadError::InvalidRange {
                turn_id: range.turn_id.clone(),
                message: message.into(),
            };
            let head: Cursor = serde_json::from_str(&range.head_cursor)
                .map_err(|_| invalid("Invalid Codex head cursor"))?;
            let tail: Option<Cursor> = range
                .tail_cursor
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(|_| invalid("Invalid Codex tail cursor"))?;
            for cursor in std::iter::once(&head).chain(tail.iter()) {
                if cursor.version != 1
                    || cursor.binding != request.source.id
                    || cursor.native_turn != head.native_turn
                    || cursor.thread.as_ref().is_some_and(|id| id != &thread)
                {
                    return Err(invalid("Codex cursor identity mismatch"));
                }
            }
            let mut file = File::open(&request.source.path).map_err(Error::from)?;
            let len = file.metadata().map_err(Error::from)?.len();
            let end = tail.as_ref().map(|cursor| cursor.offset).unwrap_or(len);
            if end < head.offset || end > len {
                return Err(invalid("Codex rollout was truncated"));
            }
            file.seek(SeekFrom::Start(head.offset))
                .map_err(Error::from)?;
            let mut reader = BufReader::new(file.take(end - head.offset));
            let mut line = String::new();
            let mut offset = head.offset;
            let mut current = None;
            let mut found_anchor = false;
            let mut calls = HashMap::<String, Value>::new();
            while reader.read_line(&mut line).map_err(Error::from)? > 0 {
                if !line.ends_with('\n') {
                    break;
                }
                let record: Value = serde_json::from_str(&line).map_err(Error::from)?;
                let item_offset = offset;
                offset += line.len() as u64;
                line.clear();
                if (record["type"] == "turn_context"
                    || record.pointer("/payload/type").and_then(Value::as_str)
                        == Some("task_started"))
                    && let Some(turn) = native_turn(&record)
                {
                    current = Some(turn);
                }
                let item_turn = native_turn(&record).or_else(|| current.clone());
                if item_turn.as_deref() != Some(&head.native_turn) {
                    continue;
                }
                found_anchor = true;
                if record["type"] != "response_item" {
                    if let Some((kind, payload)) = native_detail(&record) {
                        output.push(TurnTimelineItem {
                            turn_id: range.turn_id.clone(),
                            item: TimelineItem {
                                item_id: format!("codex:{thread}:{item_offset}"),
                                kind: "tool_result".into(),
                                raw_kind: Some(kind.into()),
                                role: "tool".into(),
                                title: Some(kind.into()),
                                status: payload["status"].as_str().map(str::to_string),
                                occurred_at: record["timestamp"].as_str().map(str::to_string),
                                content_preview: payload.to_string(),
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
                let (kind, role, content) = match raw_kind {
                    "message" => (role, role, render_content(&payload["content"])),
                    "reasoning" => ("thinking", "assistant", render_content(&payload["summary"])),
                    "function_call" | "custom_tool_call" => {
                        let name = payload["name"].as_str().unwrap_or("Tool");
                        title = Some(name.to_string());
                        if let Some(id) = payload["call_id"].as_str() {
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
                        let call = payload["call_id"].as_str().and_then(|id| calls.get(id));
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
                    turn_id: range.turn_id.clone(),
                    item: TimelineItem {
                        item_id: format!("codex:{thread}:{item_offset}"),
                        kind: kind.into(),
                        raw_kind: Some(raw_kind.into()),
                        role: role.into(),
                        title,
                        status: None,
                        occurred_at: record["timestamp"].as_str().map(str::to_string),
                        content_preview: content,
                        managed_tool_use: None,
                    },
                });
            }
            if !found_anchor && tail.is_some() {
                return Err(invalid("Codex native turn anchor cannot be restored"));
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
            "task_started"
            | "task_complete"
            | "turn_aborted"
            | "token_count"
            | "thread_settings_applied"
            | "user_message"
            | "agent_message"
            | "agent_reasoning"
            | "agent_reasoning_raw_content" => None,
            "item_completed" => {
                let item = &payload["item"];
                match item["type"].as_str()? {
                    "UserMessage" | "AgentMessage" | "Reasoning" => None,
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
