use std::{
    collections::HashSet,
    fs::File,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
};

use pontia_core::{Error, Result};
use serde_json::Value;

use super::mapping::claude_entry_to_items;
use crate::raw_transcripts::{
    CapturedTimelineBoundary, TimelineBoundaryCaptureKind, TimelineBoundaryCaptureRequest,
    TimelineBoundaryCapturer, TurnTimelineItem, TurnTimelineReadError, TurnTimelineReadRequest,
    TurnTimelineReader, read_range_from_source, source_len,
};

const CURSOR_PREFIX: &str = "claude-jsonl-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeJsonlV1Cursor {
    pub binding_id: String,
    pub byte_offset: usize,
}

impl ClaudeJsonlV1Cursor {
    pub fn encode(&self) -> String {
        format!("{CURSOR_PREFIX}:{}:{}", self.binding_id, self.byte_offset)
    }

    pub fn decode(cursor: &str, expected_binding_id: &str) -> Result<Self> {
        let mut parts = cursor.splitn(3, ':');
        if parts.next() != Some(CURSOR_PREFIX) {
            return Err(Error::Domain(
                "cursor_invalid: claude cursor format mismatch".to_string(),
            ));
        }
        let binding_id = parts.next().ok_or_else(|| {
            Error::Domain("cursor_invalid: claude cursor format mismatch".to_string())
        })?;
        if binding_id != expected_binding_id {
            return Err(Error::Domain(
                "cursor_invalid: claude cursor scope mismatch".to_string(),
            ));
        }
        let byte_offset = parts
            .next()
            .ok_or_else(|| {
                Error::Domain("cursor_invalid: claude cursor format mismatch".to_string())
            })?
            .parse()
            .map_err(|_| Error::Domain("cursor_invalid: invalid byte offset".to_string()))?;
        Ok(Self {
            binding_id: binding_id.to_string(),
            byte_offset,
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ClaudeTimelineAdapter;

impl ClaudeTimelineAdapter {
    pub const fn new() -> Self {
        Self
    }
}

impl TimelineBoundaryCapturer for ClaudeTimelineAdapter {
    fn client_type(&self) -> &'static str {
        "claude"
    }

    fn capture_boundary(
        &self,
        request: TimelineBoundaryCaptureRequest,
    ) -> Result<CapturedTimelineBoundary> {
        ensure_source(&request.source)?;
        let mut byte_offset = source_len(&request.source)?;
        if request.kind == TimelineBoundaryCaptureKind::Head {
            byte_offset =
                current_prompt_start(&request.source.path, byte_offset)?.unwrap_or(byte_offset);
        }
        Ok(CapturedTimelineBoundary {
            kind: request.kind,
            cursor: ClaudeJsonlV1Cursor {
                binding_id: request.source.id,
                byte_offset,
            }
            .encode(),
        })
    }

    fn capture_source_origin_head(
        &self,
        binding_id: &str,
        _native_entry_anchor: Option<String>,
    ) -> Result<CapturedTimelineBoundary> {
        Ok(CapturedTimelineBoundary {
            kind: TimelineBoundaryCaptureKind::Head,
            cursor: ClaudeJsonlV1Cursor {
                binding_id: binding_id.to_string(),
                byte_offset: 0,
            }
            .encode(),
        })
    }
}

impl TurnTimelineReader for ClaudeTimelineAdapter {
    fn client_type(&self) -> &'static str {
        "claude"
    }

    fn read_turn_ranges(
        &self,
        request: TurnTimelineReadRequest,
    ) -> std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError> {
        ensure_source(&request.source)?;
        let source_length = source_len(&request.source)?;
        let mut claimed_item_ids = HashSet::new();
        let mut items = Vec::new();

        for range in request.ranges {
            let head = decode_range_cursor(&range.turn_id, &range.head_cursor, &request.source.id)?;
            let (tail_offset, active) = match range.tail_cursor.as_deref() {
                Some(cursor) => (
                    decode_range_cursor(&range.turn_id, cursor, &request.source.id)?.byte_offset,
                    false,
                ),
                None => (source_length, true),
            };
            if head.byte_offset > tail_offset || tail_offset > source_length {
                return invalid_range(
                    &range.turn_id,
                    "cursor offsets are reversed or outside the source",
                );
            }

            let bytes = read_range_from_source(&request.source, head.byte_offset, tail_offset)?;
            let mut values = parse_window(&range.turn_id, &bytes, active)?;
            if !active && tail_offset < source_length {
                values.extend(read_delayed_tail_entries(
                    &request.source.path,
                    tail_offset,
                    source_length,
                    &range.turn_id,
                )?);
            }
            for value in values {
                let mapped = claude_entry_to_items(&value)
                    .map_err(|error| invalid_range_error(&range.turn_id, &error.to_string()))?;
                for item in mapped {
                    if !claimed_item_ids.insert(item.item_id.clone()) {
                        return invalid_range(&range.turn_id, "semantic Turn ranges overlap");
                    }
                    items.push(TurnTimelineItem {
                        turn_id: range.turn_id.clone(),
                        item,
                    });
                }
            }
        }
        Ok(items)
    }
}

fn ensure_source(source: &crate::raw_transcripts::ResolvedAgentBinding) -> Result<()> {
    if source.client_type != "claude" || source.format != "claude-jsonl" {
        return Err(Error::CapabilityUnavailable(
            "timeline capability unavailable for source format".to_string(),
        ));
    }
    Ok(())
}

fn current_prompt_start(path: &std::path::Path, source_length: usize) -> Result<Option<usize>> {
    if source_length == 0 {
        return Ok(None);
    }
    let mut file = File::open(path).map_err(|error| source_unavailable(path, error))?;
    let mut end = source_length;
    let mut byte = [0_u8; 1];
    while end > 0 {
        file.seek(SeekFrom::Start((end - 1) as u64))
            .map_err(|error| source_unavailable(path, error))?;
        file.read_exact(&mut byte)
            .map_err(|error| source_unavailable(path, error))?;
        if !matches!(byte[0], b'\n' | b'\r') {
            break;
        }
        end -= 1;
    }
    if end == 0 {
        return Ok(None);
    }

    const CHUNK_SIZE: usize = 8192;
    let mut start = 0;
    let mut search_end = end;
    while search_end > 0 {
        let chunk_start = search_end.saturating_sub(CHUNK_SIZE);
        let mut chunk = vec![0; search_end - chunk_start];
        file.seek(SeekFrom::Start(chunk_start as u64))
            .map_err(|error| source_unavailable(path, error))?;
        file.read_exact(&mut chunk)
            .map_err(|error| source_unavailable(path, error))?;
        if let Some(index) = chunk.iter().rposition(|byte| *byte == b'\n') {
            start = chunk_start + index + 1;
            break;
        }
        search_end = chunk_start;
    }

    let mut line = vec![0; end - start];
    file.seek(SeekFrom::Start(start as u64))
        .map_err(|error| source_unavailable(path, error))?;
    file.read_exact(&mut line)
        .map_err(|error| source_unavailable(path, error))?;
    let Ok(entry) = serde_json::from_slice::<Value>(&line) else {
        return Ok(None);
    };
    Ok(is_primary_user_entry(&entry).then_some(start))
}

// Claude invokes Stop before it durably appends the final assistant entry. For a sealed
// Turn, treat its captured tail as the start of a short read-model recovery window and
// stop before the next primary prompt so entries cannot leak into the following Turn.
fn read_delayed_tail_entries(
    path: &std::path::Path,
    start: usize,
    source_length: usize,
    turn_id: &str,
) -> std::result::Result<Vec<Value>, TurnTimelineReadError> {
    let mut file = File::open(path).map_err(|error| source_unavailable(path, error))?;
    file.seek(SeekFrom::Start(start as u64))
        .map_err(|error| source_unavailable(path, error))?;
    let mut reader = BufReader::new(file.take((source_length - start) as u64));
    let mut line = Vec::new();
    let mut values = Vec::new();

    loop {
        line.clear();
        let bytes_read = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| source_unavailable(path, error))?;
        if bytes_read == 0 || !line.ends_with(b"\n") {
            break;
        }
        line.pop();
        if line.ends_with(b"\r") {
            line.pop();
        }
        if line.is_empty() {
            continue;
        }
        let text = std::str::from_utf8(&line)
            .map_err(|_| invalid_range_error(turn_id, "timeline JSONL is not UTF-8"))?;
        let value: Value = serde_json::from_str(text)
            .map_err(|_| invalid_range_error(turn_id, "timeline JSONL is malformed"))?;
        if is_primary_user_entry(&value) {
            break;
        }
        values.push(value);
    }

    Ok(values)
}

fn is_primary_user_entry(entry: &Value) -> bool {
    let content = entry.pointer("/message/content");
    let is_primary_user_content = content.is_some_and(|content| match content {
        Value::String(_) => true,
        Value::Array(blocks) => blocks
            .iter()
            .any(|block| block.get("type").and_then(Value::as_str) != Some("tool_result")),
        _ => false,
    });
    entry.get("type").and_then(Value::as_str) == Some("user")
        && entry.get("isSidechain").and_then(Value::as_bool) != Some(true)
        && is_primary_user_content
}

fn source_unavailable(path: &std::path::Path, error: std::io::Error) -> Error {
    Error::CapabilityUnavailable(format!(
        "source_unavailable: raw source {} is unavailable: {error}",
        path.display()
    ))
}

fn decode_range_cursor(
    turn_id: &str,
    cursor: &str,
    binding_id: &str,
) -> std::result::Result<ClaudeJsonlV1Cursor, TurnTimelineReadError> {
    ClaudeJsonlV1Cursor::decode(cursor, binding_id).map_err(|_| {
        TurnTimelineReadError::InvalidRange {
            turn_id: turn_id.to_string(),
            message: "invalid or out-of-scope Claude v1 cursor".to_string(),
        }
    })
}

fn parse_window(
    turn_id: &str,
    bytes: &[u8],
    active: bool,
) -> std::result::Result<Vec<Value>, TurnTimelineReadError> {
    let complete_end = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    if !active && complete_end != bytes.len() {
        return invalid_range(turn_id, "timeline range ends with incomplete JSONL");
    }

    let mut values = Vec::new();
    for line in bytes[..complete_end].split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        let text = std::str::from_utf8(line)
            .map_err(|_| invalid_range_error(turn_id, "timeline JSONL is not UTF-8"))?;
        values.push(
            serde_json::from_str(text)
                .map_err(|_| invalid_range_error(turn_id, "timeline JSONL is malformed"))?,
        );
    }
    Ok(values)
}

fn invalid_range<T>(turn_id: &str, message: &str) -> std::result::Result<T, TurnTimelineReadError> {
    Err(invalid_range_error(turn_id, message))
}

fn invalid_range_error(turn_id: &str, message: &str) -> TurnTimelineReadError {
    TurnTimelineReadError::InvalidRange {
        turn_id: turn_id.to_string(),
        message: message.to_string(),
    }
}
