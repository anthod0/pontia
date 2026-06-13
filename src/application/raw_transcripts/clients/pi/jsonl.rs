use std::fs;

use serde_json::Value;

use crate::error::{Error, Result};

use super::super::super::{
    RawTranscriptParser, ResolvedAgentBinding, TimelineItemDetailPage, TimelineItemDetailRequest,
    TimelinePage, TimelinePageRequest,
};
use super::{
    mapping::pi_entry_to_items,
    refs::{CursorPosition, decode_pi_content_ref, decode_pi_cursor, encode_pi_cursor},
};

#[derive(Debug, Clone, Default)]
pub struct PiJsonlParser;

impl PiJsonlParser {
    pub fn new() -> Self {
        Self
    }
}

impl RawTranscriptParser for PiJsonlParser {
    fn client_type(&self) -> &'static str {
        "pi"
    }

    fn format(&self) -> &'static str {
        "pi-jsonl"
    }

    fn timeline_page(&self, request: TimelinePageRequest) -> Result<TimelinePage> {
        if request.source.client_type != self.client_type()
            || request.source.format != self.format()
        {
            return Err(Error::CapabilityUnavailable(format!(
                "unsupported source {}/{} for pi jsonl parser",
                request.source.client_type, request.source.format
            )));
        }

        let source_id = source_id(&request.source);
        let bytes = fs::read(&request.source.path).map_err(|err| {
            Error::CapabilityUnavailable(format!(
                "source_unavailable: raw source {} is unavailable: {err}",
                request.source.path.display()
            ))
        })?;
        let cursor = request
            .cursor
            .as_deref()
            .map(|cursor| decode_pi_cursor(Some(cursor), &request.source.id))
            .transpose()?;
        let upper_bound = cursor.map_or(bytes.len(), |position| position.offset);

        if upper_bound > bytes.len() {
            return Err(Error::Domain(format!(
                "cursor_invalid: offset {} exceeds source length {}",
                upper_bound,
                bytes.len()
            )));
        }

        let limit = request.limit.max(1);
        let ranges = line_ranges_until(&bytes, upper_bound)?;
        let selected_start = select_recent_round_start(&bytes, &ranges, &request.source.id, limit)?;
        let items = parse_items_in_range(&bytes, &ranges, &request.source.id, selected_start)?;
        let has_more = has_user_round_before(&bytes, &ranges, &request.source.id, selected_start)?;
        let next_cursor = has_more.then(|| {
            encode_pi_cursor(
                &request.source.id,
                CursorPosition {
                    offset: selected_start,
                    block_index: 0,
                },
            )
        });
        let tail_cursor = encode_pi_cursor(
            &request.source.id,
            CursorPosition {
                offset: bytes.len(),
                block_index: 0,
            },
        );

        Ok(TimelinePage {
            session_id: request.session_id,
            binding_id: request.source.id,
            items,
            next_cursor,
            tail_cursor: Some(tail_cursor),
            has_more,
            is_tail: request.cursor.is_none(),
            source_id,
        })
    }

    fn timeline_item_detail(
        &self,
        request: TimelineItemDetailRequest,
    ) -> Result<TimelineItemDetailPage> {
        let detail_ref = decode_pi_content_ref(&request.content_ref, &request.source.id)?;
        let bytes = fs::read(&request.source.path)?;
        if detail_ref.start > detail_ref.end || detail_ref.end > bytes.len() {
            return Err(Error::Domain(
                "content_ref_invalid: byte range outside source".to_string(),
            ));
        }
        let line = std::str::from_utf8(&bytes[detail_ref.start..detail_ref.end])
            .map_err(|err| {
                Error::Domain(format!("content_ref_invalid: source is not utf-8: {err}"))
            })?
            .trim_end_matches(['\r', '\n']);
        let entry: Value = serde_json::from_str(line)?;
        let text = match detail_ref.kind.as_str() {
            "assistant" | "thinking" | "tool_call" => entry
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(|content| content.get(detail_ref.block_index))
                .cloned()
                .unwrap_or(Value::Null),
            _ => entry.clone(),
        };
        let text = serde_json::to_string_pretty(&text)?;
        Ok(TimelineItemDetailPage {
            binding_id: request.source.id,
            content_ref: request.content_ref,
            content_type: "application/json".to_string(),
            size_bytes: text.len(),
            text,
        })
    }
}

fn line_ranges_until(bytes: &[u8], upper_bound: usize) -> Result<Vec<(usize, usize)>> {
    let mut ranges = Vec::new();
    let mut offset = 0usize;
    for line in bytes[..upper_bound].split_inclusive(|byte| *byte == b'\n') {
        let line_start = offset;
        let line_end = offset + line.len();
        offset = line_end;
        ranges.push((line_start, line_end));
    }
    if offset != upper_bound {
        return Err(Error::Domain(
            "cursor_invalid: cursor does not align with readable source boundary".to_string(),
        ));
    }
    Ok(ranges)
}

fn select_recent_round_start(
    bytes: &[u8],
    ranges: &[(usize, usize)],
    binding_id: &str,
    limit: usize,
) -> Result<usize> {
    let mut selected_start = ranges.last().map(|(start, _)| *start).unwrap_or(0);
    let mut rounds = 0usize;

    for (line_start, line_end) in ranges.iter().rev().copied() {
        let entry = parse_jsonl_entry(bytes, line_start, line_end)?;
        let produced = pi_entry_to_items(&entry, binding_id, line_start, line_end);
        if produced.is_empty() {
            continue;
        }
        selected_start = line_start;
        if produced.iter().any(|item| item.kind == "user") {
            rounds += 1;
            if rounds == limit {
                break;
            }
        }
    }

    Ok(selected_start)
}

fn parse_items_in_range(
    bytes: &[u8],
    ranges: &[(usize, usize)],
    binding_id: &str,
    selected_start: usize,
) -> Result<Vec<super::super::super::TimelineItem>> {
    let mut items = Vec::new();
    for (line_start, line_end) in ranges.iter().copied() {
        if line_start < selected_start {
            continue;
        }
        let entry = parse_jsonl_entry(bytes, line_start, line_end)?;
        items.extend(pi_entry_to_items(&entry, binding_id, line_start, line_end));
    }
    Ok(items)
}

fn has_user_round_before(
    bytes: &[u8],
    ranges: &[(usize, usize)],
    binding_id: &str,
    selected_start: usize,
) -> Result<bool> {
    for (line_start, line_end) in ranges.iter().copied() {
        if line_start >= selected_start {
            break;
        }
        let entry = parse_jsonl_entry(bytes, line_start, line_end)?;
        if pi_entry_to_items(&entry, binding_id, line_start, line_end)
            .iter()
            .any(|item| item.kind == "user")
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn parse_jsonl_entry(bytes: &[u8], start: usize, end: usize) -> Result<Value> {
    let text = std::str::from_utf8(&bytes[start..end])
        .map_err(|err| Error::Domain(format!("pi jsonl source is not utf-8: {err}")))?
        .trim_end_matches(['\r', '\n']);
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    Ok(serde_json::from_str(text)?)
}

fn source_id(source: &ResolvedAgentBinding) -> String {
    format!("{}:{}", source.client_type, source.path.display())
}
