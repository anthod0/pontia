use pontia_agent_clients::raw_transcripts::{ManagedToolUse, ManagedToolUseInput};
use pontia_core::error::{Error, Result};
use serde_json::Value;

use super::{LiveOutputIdentity, LiveOutputItem, LiveOutputUpdate};

const MAX_ITEMS_PER_STREAM: usize = 256;
const MAX_TOTAL_TEXT_BYTES: usize = 1024 * 1024;
const MAX_TOTAL_TOOL_BYTES: usize = 1024 * 1024;
const MAX_TOOL_CALL_BYTES: usize = 64 * 1024;
const MAX_ID_BYTES: usize = 256;

pub(super) fn validate_identity(identity: &LiveOutputIdentity) -> Result<()> {
    validate_non_empty("session_id", &identity.session_id)?;
    validate_non_empty("turn_id", &identity.turn_id)?;
    validate_non_empty("stream_id", &identity.stream_id)
}

pub(super) fn validate_non_empty(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(Error::Domain(format!("{field} must not be empty")));
    }
    if value.len() > MAX_ID_BYTES {
        return Err(Error::Domain(format!(
            "{field} exceeds {MAX_ID_BYTES} bytes"
        )));
    }
    Ok(())
}

pub(super) fn validate_update(update: &LiveOutputUpdate) -> Result<()> {
    match update {
        LiveOutputUpdate::AssistantTextDelta { item_id, delta } => {
            validate_non_empty("item_id", item_id)?;
            if delta.is_empty() {
                return Err(Error::Domain(
                    "assistant text delta must not be empty".into(),
                ));
            }
            if delta.len() > MAX_TOTAL_TEXT_BYTES {
                return Err(Error::Domain("assistant text delta is too large".into()));
            }
        }
        LiveOutputUpdate::ToolCall {
            item_id,
            call_id,
            tool_name,
            arguments,
            managed_tool_use,
        } => {
            validate_non_empty("item_id", item_id)?;
            validate_non_empty("call_id", call_id)?;
            validate_non_empty("tool_name", tool_name)?;
            validate_tool_call(tool_name, arguments, managed_tool_use.as_ref())?;
        }
    }
    Ok(())
}

pub(super) fn validate_items(items: &[LiveOutputItem]) -> Result<()> {
    if items.len() > MAX_ITEMS_PER_STREAM {
        return Err(Error::Domain(format!(
            "live output snapshot exceeds {MAX_ITEMS_PER_STREAM} items"
        )));
    }

    let mut total_text_bytes = 0usize;
    let mut total_tool_bytes = 0usize;
    let mut item_ids = std::collections::HashSet::new();
    let mut call_ids = std::collections::HashSet::new();
    for item in items {
        validate_non_empty("item_id", item.item_id())?;
        if !item_ids.insert(item.item_id()) {
            return Err(Error::Domain(format!(
                "duplicate live output item_id {}",
                item.item_id()
            )));
        }
        match item {
            LiveOutputItem::AssistantText { text, .. } => {
                total_text_bytes = total_text_bytes
                    .checked_add(text.len())
                    .ok_or_else(|| Error::Domain("live output text size overflow".into()))?;
            }
            LiveOutputItem::ToolCall {
                call_id,
                tool_name,
                arguments,
                managed_tool_use,
                ..
            } => {
                validate_non_empty("call_id", call_id)?;
                validate_non_empty("tool_name", tool_name)?;
                let tool_bytes =
                    validate_tool_call(tool_name, arguments, managed_tool_use.as_ref())?;
                total_tool_bytes = total_tool_bytes
                    .checked_add(tool_bytes)
                    .ok_or_else(|| Error::Domain("live output tool size overflow".into()))?;
                if !call_ids.insert(call_id) {
                    return Err(Error::Domain(format!(
                        "duplicate live output call_id {call_id}"
                    )));
                }
            }
        }
    }
    if total_text_bytes > MAX_TOTAL_TEXT_BYTES {
        return Err(Error::Domain(format!(
            "live output text exceeds {MAX_TOTAL_TEXT_BYTES} bytes"
        )));
    }
    if total_tool_bytes > MAX_TOTAL_TOOL_BYTES {
        return Err(Error::Domain(format!(
            "live output tool arguments exceed {MAX_TOTAL_TOOL_BYTES} bytes"
        )));
    }
    Ok(())
}

fn validate_tool_call(
    tool_name: &str,
    arguments: &Value,
    managed_tool_use: Option<&ManagedToolUse>,
) -> Result<usize> {
    if let Some(managed_tool_use) = managed_tool_use {
        let input_matches_tool = matches!(
            (tool_name, &managed_tool_use.input),
            ("read", ManagedToolUseInput::Read { .. })
                | ("edit", ManagedToolUseInput::Edit { .. })
                | ("write", ManagedToolUseInput::Write { .. })
                | ("bash", ManagedToolUseInput::Bash { .. })
        );
        if managed_tool_use.tool_name != tool_name || !input_matches_tool {
            return Err(Error::Domain(
                "managed tool use must match tool_name and input type".into(),
            ));
        }
    }
    let size = serde_json::to_vec(&(arguments, managed_tool_use))?.len();
    if size > MAX_TOOL_CALL_BYTES {
        return Err(Error::Domain(format!(
            "tool call payload exceeds {MAX_TOOL_CALL_BYTES} bytes"
        )));
    }
    Ok(size)
}
