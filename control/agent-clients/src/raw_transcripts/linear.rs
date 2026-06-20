use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
};

use pontia_core::{Error, Result};
use serde_json::Value;

use super::{ResolvedAgentBinding, TimelineItem, TimelinePage, TimelinePageRequest};

pub type LinearJsonlEntryMapper = fn(&Value, &str, usize, usize) -> Vec<TimelineItem>;

#[derive(Debug, Clone, Copy)]
pub struct LinearJsonlTimelineParser {
    client_type: &'static str,
    format: &'static str,
    cursor_prefix: &'static str,
    entry_to_items: LinearJsonlEntryMapper,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct LinearCursorPosition {
    offset: usize,
    block_index: usize,
}

struct RecentRoundWindow {
    base_offset: usize,
    bytes: Vec<u8>,
    has_more: bool,
}

const REVERSE_READ_CHUNK_SIZE: usize = 64 * 1024;

impl LinearJsonlTimelineParser {
    pub const fn new(
        client_type: &'static str,
        format: &'static str,
        cursor_prefix: &'static str,
        entry_to_items: LinearJsonlEntryMapper,
    ) -> Self {
        Self {
            client_type,
            format,
            cursor_prefix,
            entry_to_items,
        }
    }

    pub const fn component_name() -> &'static str {
        "linear-jsonl-timeline"
    }

    pub fn timeline_page(&self, request: TimelinePageRequest) -> Result<TimelinePage> {
        if request.source.client_type != self.client_type || request.source.format != self.format {
            return Err(Error::CapabilityUnavailable(format!(
                "unsupported source {}/{} for linear jsonl parser {}/{}",
                request.source.client_type, request.source.format, self.client_type, self.format
            )));
        }

        if request.before.is_some() && request.after.is_some() {
            return Err(Error::Domain(
                "cursor_invalid: before and after are mutually exclusive".to_string(),
            ));
        }

        let source_id = source_id(&request.source);
        let source_len = source_len(&request.source)?;

        if let Some(after) = request.after.as_deref() {
            let cursor = self.decode_cursor(after, &request.source.id)?;
            if cursor.offset > source_len {
                return Err(Error::Domain(format!(
                    "cursor_invalid: offset {} exceeds source length {}",
                    cursor.offset, source_len
                )));
            }
            let items =
                self.read_forward_items_from_source(&request.source, cursor.offset, source_len)?;
            return Ok(TimelinePage {
                session_id: request.session_id,
                binding_id: request.source.id.clone(),
                items,
                head_cursor: None,
                tail_cursor: Some(self.encode_cursor(
                    &request.source.id,
                    LinearCursorPosition {
                        offset: source_len,
                        block_index: 0,
                    },
                )),
                has_more: false,
                source_id,
            });
        }

        let cursor = request
            .before
            .as_deref()
            .map(|cursor| self.decode_cursor(cursor, &request.source.id))
            .transpose()?;
        let upper_bound = cursor.map_or(source_len, |position| position.offset);

        if upper_bound > source_len {
            return Err(Error::Domain(format!(
                "cursor_invalid: offset {} exceeds source length {}",
                upper_bound, source_len
            )));
        }

        let limit = request.limit.unwrap_or(50).max(1);
        let window = self.read_recent_round_window(&request.source, upper_bound, limit)?;
        let ranges = line_ranges_until(&window.bytes, window.base_offset, upper_bound, false)?;
        let selected_start = self.select_recent_round_start(
            &window.bytes,
            window.base_offset,
            &ranges,
            &request.source.id,
            limit,
        )?;
        let items = self.parse_items_in_range(
            &window.bytes,
            window.base_offset,
            &ranges,
            &request.source.id,
            selected_start,
        )?;
        let has_more = window.has_more;
        let head_cursor = has_more.then(|| {
            self.encode_cursor(
                &request.source.id,
                LinearCursorPosition {
                    offset: selected_start,
                    block_index: 0,
                },
            )
        });
        let tail_cursor = Some(self.encode_cursor(
            &request.source.id,
            LinearCursorPosition {
                offset: upper_bound,
                block_index: 0,
            },
        ));
        Ok(TimelinePage {
            session_id: request.session_id,
            binding_id: request.source.id,
            items,
            head_cursor,
            tail_cursor,
            has_more,
            source_id,
        })
    }

    fn read_recent_round_window(
        &self,
        source: &ResolvedAgentBinding,
        upper_bound: usize,
        limit: usize,
    ) -> Result<RecentRoundWindow> {
        let mut base_offset = upper_bound;
        let mut bytes = Vec::new();
        let mut selected_start = upper_bound;
        let mut rounds = 0usize;
        let mut has_more = false;

        while base_offset > 0 {
            let chunk_start = base_offset.saturating_sub(REVERSE_READ_CHUNK_SIZE);
            let mut chunk = read_range_from_source(source, chunk_start, base_offset)?;
            chunk.extend_from_slice(&bytes);
            bytes = chunk;
            base_offset = chunk_start;

            let ranges = line_ranges_until(&bytes, base_offset, upper_bound, base_offset > 0)?;
            rounds = 0;
            selected_start = ranges
                .last()
                .map(|(start, _)| *start)
                .unwrap_or(upper_bound);
            has_more = false;
            for (line_start, line_end) in ranges.iter().rev().copied() {
                let entry = parse_jsonl_entry(&bytes, base_offset, line_start, line_end)?;
                let produced = (self.entry_to_items)(&entry, &source.id, line_start, line_end);
                if produced.is_empty() {
                    continue;
                }
                if produced.iter().any(|item| item.kind == "user") {
                    rounds += 1;
                    if rounds == limit {
                        selected_start = line_start;
                    } else if rounds > limit {
                        has_more = true;
                        break;
                    }
                }
            }

            if has_more || base_offset == 0 {
                break;
            }
        }

        if rounds < limit {
            selected_start = line_ranges_until(&bytes, base_offset, upper_bound, base_offset > 0)?
                .first()
                .map(|(start, _)| *start)
                .unwrap_or(upper_bound);
        }

        let relative_start = selected_start.saturating_sub(base_offset);
        Ok(RecentRoundWindow {
            base_offset: selected_start,
            bytes: bytes[relative_start..].to_vec(),
            has_more,
        })
    }

    fn read_forward_items_from_source(
        &self,
        source: &ResolvedAgentBinding,
        start: usize,
        end: usize,
    ) -> Result<Vec<TimelineItem>> {
        let mut file = File::open(&source.path).map_err(|err| {
            Error::CapabilityUnavailable(format!(
                "source_unavailable: raw source {} is unavailable: {err}",
                source.path.display()
            ))
        })?;
        file.seek(SeekFrom::Start(start as u64)).map_err(|err| {
            Error::CapabilityUnavailable(format!(
                "source_unavailable: raw source {} is unavailable: {err}",
                source.path.display()
            ))
        })?;

        let mut items = Vec::new();
        let mut buffer = Vec::new();
        let mut buffer_start = start;
        let mut read_offset = start;
        let mut chunk = vec![0; REVERSE_READ_CHUNK_SIZE.min(end.saturating_sub(start).max(1))];

        while read_offset < end {
            let to_read = chunk.len().min(end - read_offset);
            file.read_exact(&mut chunk[..to_read]).map_err(|err| {
                Error::CapabilityUnavailable(format!(
                    "source_unavailable: raw source {} is unavailable: {err}",
                    source.path.display()
                ))
            })?;
            buffer.extend_from_slice(&chunk[..to_read]);
            read_offset += to_read;

            while let Some(newline_index) = buffer.iter().position(|byte| *byte == b'\n') {
                let line_end = buffer_start + newline_index + 1;
                let entry = parse_jsonl_entry(&buffer, buffer_start, buffer_start, line_end)?;
                items.extend((self.entry_to_items)(
                    &entry,
                    &source.id,
                    buffer_start,
                    line_end,
                ));
                buffer.drain(..=newline_index);
                buffer_start = line_end;
            }
        }

        if !buffer.is_empty() {
            let line_end = buffer_start + buffer.len();
            let entry = parse_jsonl_entry(&buffer, buffer_start, buffer_start, line_end)?;
            items.extend((self.entry_to_items)(
                &entry,
                &source.id,
                buffer_start,
                line_end,
            ));
        }

        Ok(items)
    }

    fn select_recent_round_start(
        &self,
        bytes: &[u8],
        base_offset: usize,
        ranges: &[(usize, usize)],
        binding_id: &str,
        limit: usize,
    ) -> Result<usize> {
        let mut selected_start = ranges.last().map(|(start, _)| *start).unwrap_or(0);
        let mut rounds = 0usize;

        for (line_start, line_end) in ranges.iter().rev().copied() {
            let entry = parse_jsonl_entry(bytes, base_offset, line_start, line_end)?;
            let produced = (self.entry_to_items)(&entry, binding_id, line_start, line_end);
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
        &self,
        bytes: &[u8],
        base_offset: usize,
        ranges: &[(usize, usize)],
        binding_id: &str,
        selected_start: usize,
    ) -> Result<Vec<TimelineItem>> {
        let mut items = Vec::new();
        for (line_start, line_end) in ranges.iter().copied() {
            if line_start < selected_start {
                continue;
            }
            let entry = parse_jsonl_entry(bytes, base_offset, line_start, line_end)?;
            items.extend((self.entry_to_items)(
                &entry, binding_id, line_start, line_end,
            ));
        }
        Ok(items)
    }

    fn encode_cursor(&self, binding_id: &str, position: LinearCursorPosition) -> String {
        format!(
            "{}:{binding_id}:{}:{}",
            self.cursor_prefix, position.offset, position.block_index
        )
    }

    fn decode_cursor(&self, cursor: &str, binding_id: &str) -> Result<LinearCursorPosition> {
        let parts: Vec<_> = cursor.split(':').collect();
        if parts.len() != 4 || parts[0] != self.cursor_prefix || parts[1] != binding_id {
            return Err(Error::Domain(
                "cursor_invalid: cursor scope mismatch".to_string(),
            ));
        }
        Ok(LinearCursorPosition {
            offset: parts[2]
                .parse()
                .map_err(|_| Error::Domain("cursor_invalid: invalid offset".to_string()))?,
            block_index: parts[3]
                .parse()
                .map_err(|_| Error::Domain("cursor_invalid: invalid block index".to_string()))?,
        })
    }
}

pub(crate) fn source_len(source: &ResolvedAgentBinding) -> Result<usize> {
    let len = fs::metadata(&source.path)
        .map_err(|err| {
            Error::CapabilityUnavailable(format!(
                "source_unavailable: raw source {} is unavailable: {err}",
                source.path.display()
            ))
        })?
        .len();
    usize::try_from(len)
        .map_err(|_| Error::Domain("source too large for this platform".to_string()))
}

pub(crate) fn read_range_from_source(
    source: &ResolvedAgentBinding,
    start: usize,
    end: usize,
) -> Result<Vec<u8>> {
    let mut file = File::open(&source.path).map_err(|err| {
        Error::CapabilityUnavailable(format!(
            "source_unavailable: raw source {} is unavailable: {err}",
            source.path.display()
        ))
    })?;
    file.seek(SeekFrom::Start(start as u64)).map_err(|err| {
        Error::CapabilityUnavailable(format!(
            "source_unavailable: raw source {} is unavailable: {err}",
            source.path.display()
        ))
    })?;
    let mut bytes = vec![0; end.saturating_sub(start)];
    file.read_exact(&mut bytes).map_err(|err| {
        Error::CapabilityUnavailable(format!(
            "source_unavailable: raw source {} is unavailable: {err}",
            source.path.display()
        ))
    })?;
    Ok(bytes)
}

fn line_ranges_until(
    bytes: &[u8],
    base_offset: usize,
    upper_bound: usize,
    skip_partial_first_line: bool,
) -> Result<Vec<(usize, usize)>> {
    let local_upper = upper_bound.saturating_sub(base_offset);
    if local_upper > bytes.len() {
        return Err(Error::Domain(
            "cursor_invalid: cursor does not align with readable source boundary".to_string(),
        ));
    }
    let mut ranges = Vec::new();
    let mut local_offset = if !skip_partial_first_line {
        0
    } else {
        bytes[..local_upper]
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|index| index + 1)
            .unwrap_or(local_upper)
    };
    while local_offset < local_upper {
        let line_start = local_offset;
        let remaining = &bytes[local_offset..local_upper];
        let line_len = remaining
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|index| index + 1)
            .unwrap_or(remaining.len());
        local_offset += line_len;
        ranges.push((base_offset + line_start, base_offset + local_offset));
    }
    Ok(ranges)
}

fn parse_jsonl_entry(bytes: &[u8], base_offset: usize, start: usize, end: usize) -> Result<Value> {
    let local_start = start.saturating_sub(base_offset);
    let local_end = end.saturating_sub(base_offset);
    let text = std::str::from_utf8(&bytes[local_start..local_end])
        .map_err(|err| Error::Domain(format!("linear jsonl source is not utf-8: {err}")))?
        .trim_end_matches(['\r', '\n']);
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    Ok(serde_json::from_str(text)?)
}

fn source_id(source: &ResolvedAgentBinding) -> String {
    format!("{}:{}", source.client_type, source.path.display())
}
