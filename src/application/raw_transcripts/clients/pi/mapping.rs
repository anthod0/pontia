use serde_json::Value;

use super::super::super::{ManagedToolUse, TimelineItem, ToolUseParser};
use super::{refs::encode_pi_content_ref, tool_use::PiToolUseParser};

pub(super) fn pi_entry_to_items(
    entry: &Value,
    binding_id: &str,
    start: usize,
    end: usize,
) -> Vec<TimelineItem> {
    if entry.get("type").and_then(Value::as_str).is_some()
        && entry.get("id").and_then(Value::as_str).is_none()
    {
        eprintln!("pi transcript entry at byte {start} missing stable id; skipping");
        return Vec::new();
    }
    match entry.get("type").and_then(Value::as_str) {
        Some("message") => pi_message_entry_to_items(entry, binding_id, start, end),
        Some("model_change") => vec![timeline_item(
            binding_id,
            entry,
            start,
            end,
            0,
            "model_change",
            "system",
            model_change_title(entry),
            None,
            model_change_title(entry).unwrap_or_default(),
            None,
        )],
        _ => Vec::new(),
    }
}

fn pi_message_entry_to_items(
    entry: &Value,
    binding_id: &str,
    start: usize,
    end: usize,
) -> Vec<TimelineItem> {
    let Some(message) = entry.get("message") else {
        return Vec::new();
    };
    match message.get("role").and_then(Value::as_str) {
        Some("user") => vec![timeline_item(
            binding_id,
            entry,
            start,
            end,
            0,
            "user",
            "user",
            None,
            None,
            content_preview(message.get("content")),
            None,
        )],
        Some("assistant") => message
            .get("content")
            .and_then(Value::as_array)
            .map(|blocks| {
                blocks
                    .iter()
                    .enumerate()
                    .filter_map(|(block_index, block)| {
                        assistant_block_item(entry, binding_id, start, end, block_index, block)
                    })
                    .collect()
            })
            .unwrap_or_default(),
        Some("toolResult") => vec![timeline_item(
            binding_id,
            entry,
            start,
            end,
            0,
            "toolResult",
            "tool",
            message
                .get("toolName")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            Some(
                if message.get("isError").and_then(Value::as_bool) == Some(true) {
                    "error".to_string()
                } else {
                    "completed".to_string()
                },
            ),
            content_preview(message.get("content")),
            None,
        )],
        Some("bashExecution") => vec![timeline_item(
            binding_id,
            entry,
            start,
            end,
            0,
            "bashExecution",
            "user",
            message
                .get("command")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            None,
            message
                .get("output")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            None,
        )],
        Some("custom") => vec![timeline_item(
            binding_id,
            entry,
            start,
            end,
            0,
            "custom",
            "system",
            message
                .get("customType")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            None,
            content_preview(message.get("content")),
            None,
        )],
        Some("branchSummary") => vec![timeline_item(
            binding_id,
            entry,
            start,
            end,
            0,
            "branchSummary",
            "system",
            None,
            None,
            message
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            None,
        )],
        Some("compactionSummary") => vec![timeline_item(
            binding_id,
            entry,
            start,
            end,
            0,
            "compactionSummary",
            "system",
            None,
            None,
            message
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            None,
        )],
        Some(raw_kind) => vec![timeline_item(
            binding_id,
            entry,
            start,
            end,
            0,
            raw_kind,
            "system",
            Some(raw_kind.to_string()),
            None,
            content_preview(message.get("content")),
            None,
        )],
        _ => Vec::new(),
    }
}

fn assistant_block_item(
    entry: &Value,
    binding_id: &str,
    start: usize,
    end: usize,
    block_index: usize,
    block: &Value,
) -> Option<TimelineItem> {
    match block.get("type").and_then(Value::as_str) {
        Some("text") => Some(timeline_item(
            binding_id,
            entry,
            start,
            end,
            block_index,
            "text",
            "assistant",
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
            binding_id,
            entry,
            start,
            end,
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
        Some("toolCall") => Some(timeline_item(
            binding_id,
            entry,
            start,
            end,
            block_index,
            "toolCall",
            "tool",
            block
                .get("name")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            Some("started".to_string()),
            tool_call_preview(block),
            managed_tool_use_from_tool_call(block),
        )),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn timeline_item(
    binding_id: &str,
    entry: &Value,
    start: usize,
    end: usize,
    block_index: usize,
    raw_kind: &str,
    role: &str,
    title: Option<String>,
    status: Option<String>,
    preview: String,
    managed_tool_use: Option<ManagedToolUse>,
) -> TimelineItem {
    let item_id = timeline_item_id(entry, block_index)
        .expect("pi_entry_to_items filters entries without stable ids before mapping");
    let kind = normalize_pi_timeline_kind(raw_kind);
    TimelineItem {
        item_id,
        kind: kind.to_string(),
        raw_kind: Some(raw_kind.to_string()),
        role: role.to_string(),
        title,
        status,
        occurred_at: entry
            .get("timestamp")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        content_preview: timeline_content_preview(kind, preview),
        content_ref: encode_pi_content_ref(binding_id, start, end, block_index, kind),
        managed_tool_use,
    }
}

fn managed_tool_use_from_tool_call(block: &Value) -> Option<ManagedToolUse> {
    let name = block.get("name").and_then(Value::as_str)?;
    let arguments = block.get("arguments").unwrap_or(&Value::Null);
    PiToolUseParser.parse_tool_use(name, arguments)
}

fn timeline_item_id(entry: &Value, block_index: usize) -> Option<String> {
    let entry_id = entry.get("id").and_then(Value::as_str)?;
    Some(format!("pi:entry:{entry_id}:block:{block_index}"))
}

fn normalize_pi_timeline_kind(raw_kind: &str) -> &str {
    match raw_kind {
        "user" => "user",
        "text" => "assistant",
        "thinking" => "thinking",
        "toolCall" => "tool_call",
        "toolResult" => "tool_result",
        "model_change" => "model_change",
        other => other,
    }
}

fn content_preview(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|block| {
                block
                    .get("text")
                    .or_else(|| block.get("thinking"))
                    .and_then(Value::as_str)
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

fn tool_call_preview(block: &Value) -> String {
    let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
    let args = block.get("arguments").cloned().unwrap_or(Value::Null);
    format!("{name} {args}")
}

fn model_change_title(entry: &Value) -> Option<String> {
    let provider = entry.get("provider").and_then(Value::as_str);
    let model = entry.get("modelId").and_then(Value::as_str);
    match (provider, model) {
        (Some(provider), Some(model)) => Some(format!("{provider}/{model}")),
        (None, Some(model)) => Some(model.to_string()),
        _ => None,
    }
}

fn timeline_content_preview(kind: &str, text: String) -> String {
    match kind {
        "user" | "assistant" => text,
        _ => truncate_preview(&text),
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
