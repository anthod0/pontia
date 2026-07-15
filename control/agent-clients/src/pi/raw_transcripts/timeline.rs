use std::collections::{HashMap, HashSet};

use pontia_core::{Error, Result};
use serde_json::Value;

use crate::raw_transcripts::{
    CapturedTimelineBoundary, TimelineBoundaryCaptureKind, TimelineBoundaryCaptureRequest,
    TimelineBoundaryCapturer, TurnTimelineItem, TurnTimelineReadError, TurnTimelineReadRequest,
    TurnTimelineReader, read_range_from_source, source_len,
};

use super::mapping::pi_entry_to_items;

const CURSOR_PREFIX: &str = "pi-jsonl-v2";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineBoundaryRelation {
    After,
}

impl TimelineBoundaryRelation {
    fn as_str(self) -> &'static str {
        match self {
            Self::After => "after",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiJsonlV2Cursor {
    pub binding_id: String,
    pub byte_offset: usize,
    pub native_entry_anchor: Option<String>,
    pub relation: TimelineBoundaryRelation,
}

impl PiJsonlV2Cursor {
    pub fn encode(&self) -> String {
        format!(
            "{CURSOR_PREFIX}:{}:{}:{}:{}",
            self.binding_id,
            self.byte_offset,
            self.relation.as_str(),
            self.native_entry_anchor.as_deref().unwrap_or_default()
        )
    }

    pub fn decode(cursor: &str, expected_binding_id: &str) -> Result<Self> {
        let parts: Vec<_> = cursor.splitn(5, ':').collect();
        if parts.len() != 5 || parts[0] != CURSOR_PREFIX {
            return Err(Error::Domain(
                "cursor_invalid: pi cursor format mismatch".to_string(),
            ));
        }
        if parts[1] != expected_binding_id {
            return Err(Error::Domain(
                "cursor_invalid: pi cursor scope mismatch".to_string(),
            ));
        }
        let byte_offset = parts[2]
            .parse()
            .map_err(|_| Error::Domain("cursor_invalid: invalid byte offset".to_string()))?;
        let relation = match parts[3] {
            "after" => TimelineBoundaryRelation::After,
            _ => {
                return Err(Error::Domain(
                    "cursor_invalid: invalid boundary relation".to_string(),
                ));
            }
        };
        let native_entry_anchor = (!parts[4].is_empty()).then(|| parts[4].to_string());
        Ok(Self {
            binding_id: parts[1].to_string(),
            byte_offset,
            native_entry_anchor,
            relation,
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PiTimelineAdapter;

impl PiTimelineAdapter {
    pub const fn new() -> Self {
        Self
    }
}

impl TimelineBoundaryCapturer for PiTimelineAdapter {
    fn client_type(&self) -> &'static str {
        "pi"
    }

    fn capture_boundary(
        &self,
        request: TimelineBoundaryCaptureRequest,
    ) -> Result<CapturedTimelineBoundary> {
        if request.source.client_type != "pi" || request.source.format != "pi-jsonl" {
            return Err(Error::CapabilityUnavailable(format!(
                "unsupported source {}/{} for pi timeline adapter",
                request.source.client_type, request.source.format
            )));
        }
        let missing_anchor_allowed = request.kind == TimelineBoundaryCaptureKind::Head
            && request.allow_missing_native_entry_anchor;
        if request.native_entry_anchor.is_none() && !missing_anchor_allowed {
            return Err(Error::Domain(
                "cursor_invalid: native entry anchor is required".to_string(),
            ));
        }
        let cursor = PiJsonlV2Cursor {
            binding_id: request.source.id.clone(),
            byte_offset: source_len(&request.source)?,
            native_entry_anchor: request.native_entry_anchor,
            relation: TimelineBoundaryRelation::After,
        }
        .encode();
        Ok(CapturedTimelineBoundary {
            kind: request.kind,
            cursor,
        })
    }
}

struct ParsedEntry {
    value: Value,
    parent_id: Option<String>,
    start: usize,
    end: usize,
}

impl TurnTimelineReader for PiTimelineAdapter {
    fn client_type(&self) -> &'static str {
        "pi"
    }

    fn read_turn_ranges(
        &self,
        request: TurnTimelineReadRequest,
    ) -> std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError> {
        if request.source.client_type != "pi" || request.source.format != "pi-jsonl" {
            return Err(Error::CapabilityUnavailable(
                "timeline capability unavailable for source format".to_string(),
            )
            .into());
        }

        let source_length = source_len(&request.source)?;
        let mut claimed_entry_ids = HashSet::new();
        let mut items = Vec::new();
        for range in request.ranges {
            let head = decode_range_cursor(&range.turn_id, &range.head_cursor, &request.source.id)?;
            if head.native_entry_anchor.is_none() && range.turn_index != 1 {
                return invalid_range(
                    &range.turn_id,
                    "only the first Session Turn may have a null head anchor",
                );
            }
            let (tail_offset, terminal_id, is_active) = match range.tail_cursor.as_deref() {
                Some(tail_cursor) => {
                    let tail =
                        decode_range_cursor(&range.turn_id, tail_cursor, &request.source.id)?;
                    let Some(terminal_id) = tail.native_entry_anchor else {
                        return invalid_range(
                            &range.turn_id,
                            "terminal native entry anchor is missing",
                        );
                    };
                    (tail.byte_offset, Some(terminal_id), false)
                }
                None => (source_length, None, true),
            };
            if head.byte_offset > tail_offset || tail_offset > source_length {
                return invalid_range(
                    &range.turn_id,
                    "cursor offsets are reversed or outside the source",
                );
            }

            let bytes = read_range_from_source(&request.source, head.byte_offset, tail_offset)?;
            let (parsed, record_count) = parse_window(&range.turn_id, &bytes, head.byte_offset)?;
            if is_active && record_count == 0 {
                continue;
            }
            if is_active && parsed.len() != record_count {
                return invalid_range(
                    &range.turn_id,
                    "active timeline contains an entry without a native id",
                );
            }
            let mut by_id = HashMap::new();
            for (index, (entry_id, _)) in parsed.iter().enumerate() {
                if by_id.insert(entry_id.as_str(), index).is_some() {
                    return invalid_range(&range.turn_id, "duplicate native entry id in range");
                }
            }

            let mut chain = Vec::new();
            let terminal_id = terminal_id
                .as_deref()
                .or_else(|| parsed.last().map(|(entry_id, _)| entry_id.as_str()))
                .expect("an active range with no parsed entries was handled above");
            let mut current = terminal_id;
            let mut visited = HashSet::new();
            loop {
                if !visited.insert(current.to_string()) {
                    return invalid_range(
                        &range.turn_id,
                        "native entry parent chain contains a cycle",
                    );
                }
                let Some(index) = by_id.get(current).copied() else {
                    return invalid_range(&range.turn_id, "native entry parent chain is broken");
                };
                chain.push(index);
                let parent = parsed[index].1.parent_id.as_deref();
                if parent == head.native_entry_anchor.as_deref() {
                    break;
                }
                let Some(parent) = parent else {
                    return invalid_range(
                        &range.turn_id,
                        "native entry parent chain does not reach the head anchor",
                    );
                };
                current = parent;
            }
            chain.reverse();
            if is_active && chain.len() != parsed.len() {
                return invalid_range(
                    &range.turn_id,
                    "active timeline entries do not form one consecutive parent chain",
                );
            }

            for index in chain {
                let (entry_id, entry) = &parsed[index];
                if !claimed_entry_ids.insert(entry_id.clone()) {
                    return invalid_range(&range.turn_id, "semantic Turn ranges overlap");
                }
                items.extend(
                    pi_entry_to_items(&entry.value, &request.source.id, entry.start, entry.end)
                        .into_iter()
                        .map(|item| TurnTimelineItem {
                            turn_id: range.turn_id.clone(),
                            item,
                        }),
                );
            }
        }
        Ok(items)
    }
}

fn decode_range_cursor(
    turn_id: &str,
    cursor: &str,
    binding_id: &str,
) -> std::result::Result<PiJsonlV2Cursor, TurnTimelineReadError> {
    PiJsonlV2Cursor::decode(cursor, binding_id).map_err(|_| TurnTimelineReadError::InvalidRange {
        turn_id: turn_id.to_string(),
        message: "invalid or out-of-scope Pi v2 cursor".to_string(),
    })
}

fn parse_window(
    turn_id: &str,
    bytes: &[u8],
    base_offset: usize,
) -> std::result::Result<(Vec<(String, ParsedEntry)>, usize), TurnTimelineReadError> {
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        return invalid_range(turn_id, "timeline range ends with incomplete JSONL");
    }
    let mut parsed = Vec::new();
    let mut record_count = 0;
    let mut local_start = 0;
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        record_count += 1;
        let local_end = local_start + line.len();
        let text = std::str::from_utf8(line)
            .map_err(|_| invalid_range_error(turn_id, "timeline JSONL is not UTF-8"))?
            .trim_end_matches(['\r', '\n']);
        if text.trim().is_empty() {
            return invalid_range(turn_id, "timeline JSONL contains an empty record");
        }
        let value: Value = serde_json::from_str(text)
            .map_err(|_| invalid_range_error(turn_id, "timeline JSONL is malformed"))?;
        if let Some(entry_id) = value.get("id").and_then(Value::as_str) {
            let parent_id = match value.get("parentId") {
                Some(Value::String(parent_id)) => Some(parent_id.clone()),
                Some(Value::Null) => None,
                _ => return invalid_range(turn_id, "native entry has an invalid parentId"),
            };
            parsed.push((
                entry_id.to_string(),
                ParsedEntry {
                    value,
                    parent_id,
                    start: base_offset + local_start,
                    end: base_offset + local_end,
                },
            ));
        }
        local_start = local_end;
    }
    Ok((parsed, record_count))
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
