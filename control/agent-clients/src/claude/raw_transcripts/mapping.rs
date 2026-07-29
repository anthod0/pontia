use pontia_core::Error;
use serde_json::Value;

use super::tool_use::ClaudeToolUseParser;
use crate::raw_transcripts::{ManagedToolUse, TimelineItem, ToolUseParser};

pub(super) fn claude_entry_to_items(entry: &Value) -> Result<Vec<TimelineItem>, Error> {
    if entry.get("isSidechain").and_then(Value::as_bool) == Some(true) {
        return Ok(Vec::new());
    }

    match entry.get("type").and_then(Value::as_str) {
        Some("user") => message_items(entry, "user"),
        Some("assistant") => message_items(entry, "assistant"),
        _ => Ok(Vec::new()),
    }
}

fn message_items(entry: &Value, role: &str) -> Result<Vec<TimelineItem>, Error> {
    let Some(content) = entry.pointer("/message/content") else {
        return Ok(Vec::new());
    };
    let entry_id = entry.get("uuid").and_then(Value::as_str).ok_or_else(|| {
        Error::Domain("claude transcript message is missing a stable uuid".to_string())
    })?;

    match content {
        Value::String(text) => Ok(vec![timeline_item(
            entry,
            entry_id,
            0,
            role,
            role,
            None,
            None,
            text.clone(),
            None,
        )]),
        Value::Array(blocks) => Ok(blocks
            .iter()
            .enumerate()
            .filter_map(|(index, block)| block_item(entry, entry_id, index, block, role))
            .collect()),
        _ => Ok(Vec::new()),
    }
}

fn block_item(
    entry: &Value,
    entry_id: &str,
    block_index: usize,
    block: &Value,
    message_role: &str,
) -> Option<TimelineItem> {
    match block.get("type").and_then(Value::as_str) {
        Some("text") => Some(timeline_item(
            entry,
            entry_id,
            block_index,
            if message_role == "user" {
                "user"
            } else {
                "text"
            },
            message_role,
            None,
            None,
            block
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            None,
        )),
        Some("thinking") => Some(timeline_item(
            entry,
            entry_id,
            block_index,
            "thinking",
            "assistant",
            None,
            None,
            block
                .get("thinking")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            None,
        )),
        Some("tool_use") => {
            let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
            let input = block.get("input").unwrap_or(&Value::Null);
            Some(timeline_item(
                entry,
                entry_id,
                block_index,
                "tool_use",
                "tool",
                Some(name.to_string()),
                Some("started".to_string()),
                format!("{name} {input}"),
                ClaudeToolUseParser.parse_tool_use(name, input),
            ))
        }
        Some("tool_result") => Some(timeline_item(
            entry,
            entry_id,
            block_index,
            "tool_result",
            "tool",
            None,
            Some(
                if block.get("is_error").and_then(Value::as_bool) == Some(true) {
                    "error"
                } else {
                    "completed"
                }
                .to_string(),
            ),
            content_preview(block.get("content")),
            None,
        )),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn timeline_item(
    entry: &Value,
    entry_id: &str,
    block_index: usize,
    raw_kind: &str,
    role: &str,
    title: Option<String>,
    status: Option<String>,
    preview: String,
    managed_tool_use: Option<ManagedToolUse>,
) -> TimelineItem {
    let kind = normalize_kind(raw_kind);
    TimelineItem {
        item_id: format!("claude:entry:{entry_id}:block:{block_index}"),
        kind: kind.to_string(),
        raw_kind: Some(raw_kind.to_string()),
        role: role.to_string(),
        title,
        status,
        occurred_at: entry
            .get("timestamp")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        content_preview: if matches!(kind, "user" | "assistant") {
            preview
        } else {
            truncate_preview(&preview)
        },
        managed_tool_use,
    }
}

fn normalize_kind(raw_kind: &str) -> &str {
    match raw_kind {
        "text" => "assistant",
        "tool_use" => "tool_call",
        other => other,
    }
}

fn content_preview(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

fn truncate_preview(text: &str) -> String {
    const MAX_CHARS: usize = 240;
    let mut chars = text.chars();
    let preview: String = chars.by_ref().take(MAX_CHARS).collect();
    if chars.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}
