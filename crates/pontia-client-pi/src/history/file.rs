//! Entry-addressed reads of Pi's persisted append-ordered JSONL tree.
use super::{MAX_ENTRIES, MAX_SNAPSHOT_BYTES, PiEntryCursor, identity, invalid, unavailable};
use crate::raw_transcripts::{PiJsonlV2Cursor, mapping::pi_entry_to_items};
use pontia_application::client_contract::raw_transcripts::{
    AgentBindingResolveRequest, SessionTimelineSelection, TurnTimelineItem, TurnTimelineRange,
    TurnTimelineReadError,
};
use pontia_core::Result;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

type ReadResult<T> = std::result::Result<T, TurnTimelineReadError>;

// Dropping an HTTP read must also stop its blocking file scan, not just detach it.
struct CancelOnDrop(Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}
pub(super) async fn run<T: Send + 'static>(
    binding: AgentBindingResolveRequest,
    read: impl FnOnce(&AgentBindingResolveRequest, &AtomicBool) -> ReadResult<T> + Send + 'static,
) -> ReadResult<T> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let guard = CancelOnDrop(cancelled.clone());
    let result = tokio::task::spawn_blocking(move || read(&binding, &cancelled))
        .await
        .map_err(unavailable)?;
    drop(guard);
    result
}

struct ReverseLines {
    file: File,
    position: u64,
    block: Vec<u8>,
    pending: Vec<u8>,
    length: u64,
    modified: Option<std::time::SystemTime>,
}
impl ReverseLines {
    fn open(binding: &AgentBindingResolveRequest) -> Result<Self> {
        let path = binding
            .client_session_file
            .as_ref()
            .ok_or_else(|| unavailable("missing Pi session file"))?;
        let file = File::open(path).map_err(unavailable)?;
        let metadata = file.metadata().map_err(unavailable)?;
        let mut reader = BufReader::new((&file).take(MAX_SNAPSHOT_BYTES as u64 + 1));
        let mut header = Vec::new();
        reader.read_until(b'\n', &mut header).map_err(unavailable)?;
        if header.len() > MAX_SNAPSHOT_BYTES || !header.ends_with(b"\n") {
            return Err(unavailable("invalid Pi session header"));
        }
        let header: Value = serde_json::from_slice(&header).map_err(unavailable)?;
        if header["type"] != "session" || header["id"].as_str() != Some(&binding.client_session_key)
        {
            return Err(unavailable("Pi session file identity mismatch"));
        }
        Ok(Self {
            file,
            position: metadata.len(),
            length: metadata.len(),
            modified: metadata.modified().ok(),
            block: Vec::new(),
            pending: Vec::new(),
        })
    }
    fn next(&mut self, cancelled: &AtomicBool) -> Result<Option<(u64, Vec<u8>)>> {
        loop {
            if cancelled.load(Ordering::Relaxed) {
                return Err(unavailable("history read cancelled"));
            }
            if let Some(byte) = self.block.pop() {
                if byte == b'\n' {
                    if self.pending.is_empty() {
                        continue;
                    }
                    self.pending.reverse();
                    let line = std::mem::take(&mut self.pending);
                    return Ok(Some((self.position + self.block.len() as u64 + 1, line)));
                }
                self.pending.push(byte);
                if self.pending.len() > MAX_SNAPSHOT_BYTES {
                    return Err(unavailable("Pi entry exceeds snapshot budget"));
                }
            } else if self.position == 0 {
                if self.pending.is_empty() {
                    return Ok(None);
                }
                self.pending.reverse();
                return Ok(Some((0, std::mem::take(&mut self.pending))));
            } else {
                let size = self.position.min(16 * 1024) as usize;
                self.position -= size as u64;
                self.file
                    .seek(SeekFrom::Start(self.position))
                    .map_err(unavailable)?;
                self.block.resize(size, 0);
                self.file.read_exact(&mut self.block).map_err(unavailable)?;
                // A partial final record is not a valid persisted snapshot.
                if self.position + size as u64 == self.length && self.block.last() != Some(&b'\n') {
                    return Err(unavailable("Pi session file ends with an incomplete entry"));
                }
            }
        }
    }
    fn validate(&self) -> Result<()> {
        let metadata = self.file.metadata().map_err(unavailable)?;
        if metadata.len() != self.length || metadata.modified().ok() != self.modified {
            return Err(unavailable("Pi session file changed during history read"));
        }
        Ok(())
    }
}

struct Range {
    id: String,
    head: Option<String>,
    target: Option<String>,
    started: bool,
    done: bool,
    selected: bool,
    usable: bool,
    visited: HashSet<String>,
}
impl Range {
    fn error(&self, message: &str) -> TurnTimelineReadError {
        TurnTimelineReadError::InvalidRange {
            turn_id: self.id.clone(),
            message: message.into(),
        }
    }
    fn visit(&mut self, entry: &Value) -> ReadResult<bool> {
        if self.done {
            return Ok(false);
        }
        let id = entry["id"].as_str().unwrap();
        if !self.started && self.target.is_none() {
            // On disk, Pi rebuilds its leaf from the last persisted entry.
            self.target = Some(id.into());
        }
        if self.target.as_deref() != Some(id) {
            return Ok(false);
        }
        if !self.visited.insert(id.to_owned()) || entry["parentId"].as_str() == Some(id) {
            return Err(self.error("native branch contains a cycle or duplicate identity"));
        }
        if self.visited.len() > MAX_ENTRIES {
            return Err(unavailable("history exceeds entry budget").into());
        }
        self.started = true;
        if self.head.as_deref() == Some(id) {
            self.done = true;
            return Ok(false);
        }
        self.target = entry["parentId"].as_str().map(str::to_owned);
        if self.target.is_none() {
            if self.head.is_some() {
                return Err(self.error("Turn head is not an ancestor of tail"));
            }
            self.done = true;
        }
        Ok(true)
    }
}

fn anchor(binding: &AgentBindingResolveRequest, cursor: &str) -> Result<Option<String>> {
    if PiEntryCursor::is_entry(cursor) {
        Ok(PiEntryCursor::decode(cursor, &binding.id, Some(&binding.client_session_key))?.anchor)
    } else {
        Ok(PiJsonlV2Cursor::decode(cursor, &binding.id)?.native_entry_anchor)
    }
}

fn ranges(
    binding: &AgentBindingResolveRequest,
    selection: &SessionTimelineSelection,
) -> ReadResult<Vec<Range>> {
    let mut ranges = Vec::new();
    for (index, turn) in selection.turns.iter().enumerate() {
        let selected = selection.selected_turn_ids.contains(&turn.turn_id);
        let decode = || -> Result<Range> {
            let head = anchor(
                binding,
                turn.head_cursor
                    .as_deref()
                    .ok_or_else(|| invalid("Turn head missing"))?,
            )?;
            if head.is_none() && index != 0 {
                return Err(invalid("source origin is only valid for the first Turn"));
            }
            // Page ownership depends on a valid head even when an unselected
            // Turn has an unusable tail. Keep these two concerns independent.
            let tail = turn
                .tail_cursor
                .as_deref()
                .map(|c| anchor(binding, c))
                .transpose()
                .and_then(|tail| {
                    if tail.as_ref().is_some_and(Option::is_none) {
                        Err(invalid("terminal entry anchor missing"))
                    } else {
                        Ok(tail)
                    }
                });
            let (target, usable) = match tail {
                Ok(tail) => (tail.flatten(), selected || turn.tail_cursor.is_some()),
                Err(error) if selected => return Err(error),
                Err(_) => (None, false),
            };
            Ok(Range {
                id: turn.turn_id.clone(),
                head,
                target,
                started: false,
                done: !usable,
                selected,
                usable,
                visited: HashSet::new(),
            })
        };
        match decode() {
            Ok(range) => ranges.push(range),
            Err(error) if selected => {
                return Err(TurnTimelineReadError::InvalidRange {
                    turn_id: turn.turn_id.clone(),
                    message: error.to_string(),
                });
            }
            Err(_) => {}
        }
    }
    Ok(ranges)
}

/// One reverse pass follows all requested parent chains. Unrelated entry payloads
/// are discarded immediately; older records need not be read once the heads match.
fn scan(
    binding: &AgentBindingResolveRequest,
    mut ranges: Vec<Range>,
    checkpoints: bool,
    cancelled: &AtomicBool,
) -> ReadResult<Vec<(u64, Option<String>, Value)>> {
    let mut lines = ReverseLines::open(binding)?;
    let mut active = Range {
        id: "session".into(),
        head: None,
        target: None,
        started: false,
        done: false,
        selected: false,
        usable: true,
        visited: HashSet::new(),
    };
    let mut starts = HashMap::new();
    let mut retained = Vec::new();
    let mut retained_bytes = 0usize;
    let mut overlaps = HashSet::new();
    let mut needed_unselected = HashSet::new();
    let mut saw_entry = false;
    while let Some((position, bytes)) = lines.next(cancelled)? {
        let entry: Value = serde_json::from_slice(&bytes).map_err(unavailable)?;
        if entry["type"] == "session" {
            if position != 0 || (checkpoints && active.started && !active.done) {
                return Err(unavailable("invalid persisted native branch").into());
            }
            break;
        }
        saw_entry = true;
        let id = entry["id"]
            .as_str()
            .filter(|s| identity(s))
            .ok_or_else(|| unavailable("missing native entry identity"))?;
        if !entry["type"].as_str().is_some_and(identity)
            || !entry["timestamp"].as_str().is_some_and(identity)
            || !matches!(&entry["parentId"], Value::Null | Value::String(_))
            || !entry
                .as_object()
                .is_some_and(|o| o.contains_key("parentId"))
        {
            return Err(unavailable("invalid native entry fields").into());
        }
        let at_head = ranges.iter().any(|r| r.head.as_deref() == Some(id));
        let on_active = checkpoints && active.visit(&entry)?;
        let mut owners = Vec::new();
        for (index, range) in ranges.iter_mut().enumerate() {
            if range.head.as_deref() == Some(id) {
                starts.insert(range.id.clone(), position);
            }
            match range.visit(&entry) {
                Ok(true) => owners.push(index),
                Err(error) if range.selected => return Err(error),
                Err(_) => {
                    range.done = true;
                    range.usable = false;
                }
                _ => {}
            }
        }
        let selected_owner = owners.iter().any(|&index| ranges[index].selected);
        let checkpoint = on_active
            && (entry["type"] == "compaction"
                || (entry["type"] == "message" && entry["message"]["role"] == "compactionSummary"));
        for (offset, &left) in owners.iter().enumerate() {
            for &right in &owners[offset + 1..] {
                overlaps.insert((left, right));
            }
            if !ranges[left].selected && (checkpoint || owners.len() > 1) {
                needed_unselected.insert(left);
            }
        }
        if selected_owner || checkpoint {
            retained_bytes = retained_bytes
                .checked_add(bytes.len())
                .ok_or_else(|| unavailable("history exceeds snapshot budget"))?;
            if retained_bytes > MAX_SNAPSHOT_BYTES || retained.len() >= MAX_ENTRIES {
                return Err(unavailable("history exceeds snapshot budget").into());
            }
            retained.push((position, None, entry));
        }
        if ranges.iter().any(|r| r.selected) && ranges.iter().filter(|r| r.selected).all(|r| r.done)
        {
            // Gap checkpoints immediately BEFORE a requested head belong to
            // that page too. Scan through the preceding Turn's start boundary.
            let earliest_head = ranges
                .iter()
                .filter(|r| r.selected)
                .map(|r| {
                    r.head
                        .as_ref()
                        .map_or(0, |_| starts.get(&r.id).copied().unwrap_or(0))
                })
                .min()
                .unwrap_or(0);
            if (!checkpoints || (position < earliest_head && at_head))
                && needed_unselected.iter().all(|&index| ranges[index].done)
            {
                break;
            }
        }
    }
    if !saw_entry {
        for range in &mut ranges {
            if range.head.is_none() && range.target.is_none() {
                range.done = true;
            }
        }
    }
    for range in &ranges {
        if range.selected && !range.done {
            return Err(range.error("native branch anchor missing"));
        }
    }
    let valid = |index: usize| ranges[index].usable && ranges[index].done;
    if overlaps
        .iter()
        .any(|&(left, right)| valid(left) && valid(right))
    {
        return Err(invalid("overlapping Turn ranges").into());
    }
    // Commit ownership only after the whole chain is validated. An invalid,
    // unselected range must not fail a valid page or hide its checkpoints.
    for (_, owner, entry) in &mut retained {
        let id = entry["id"].as_str().unwrap();
        *owner = ranges
            .iter()
            .enumerate()
            .find(|(index, range)| {
                valid(*index) && range.head.as_deref() != Some(id) && range.visited.contains(id)
            })
            .map(|(_, range)| range.id.clone());
    }
    if checkpoints {
        // A head is excluded from its Turn, but a checkpoint at that head belongs
        // to the following page. Origin heads sort before every native entry.
        let mut starts: Vec<_> = ranges
            .iter()
            .filter_map(|r| {
                r.head.as_ref().map_or(Some((0, &r.id)), |_| {
                    starts.get(&r.id).map(|p| (p + 1, &r.id))
                })
            })
            .collect();
        starts.sort_by_key(|(position, _)| *position);
        retained.retain(|(position, owner, _)| {
            if let Some(owner) = owner {
                return ranges.iter().any(|r| r.selected && &r.id == owner);
            }
            ranges.is_empty()
                || starts
                    .iter()
                    .find(|(start, _)| start > position)
                    .or_else(|| starts.last())
                    .is_some_and(|(_, id)| ranges.iter().any(|r| r.selected && &r.id == *id))
        });
    }
    lines.validate()?;
    retained.reverse();
    Ok(retained)
}

pub(super) fn read_session(
    binding: &AgentBindingResolveRequest,
    selection: SessionTimelineSelection,
    cancelled: &AtomicBool,
) -> ReadResult<Vec<TurnTimelineItem>> {
    let ranges = ranges(binding, &selection)?;
    Ok(scan(binding, ranges, true, cancelled)?
        .into_iter()
        .flat_map(|(_, turn_id, entry)| {
            pi_entry_to_items(&entry, 0)
                .into_iter()
                .map(move |item| TurnTimelineItem {
                    turn_id: turn_id.clone(),
                    item,
                })
        })
        .collect())
}

pub(super) fn read_ranges(
    binding: &AgentBindingResolveRequest,
    ranges: Vec<TurnTimelineRange>,
    cancelled: &AtomicBool,
) -> ReadResult<Vec<TurnTimelineItem>> {
    Ok(read_entries(binding, ranges, cancelled)?
        .into_iter()
        .flat_map(|(turn_id, entry)| {
            pi_entry_to_items(&entry, 0)
                .into_iter()
                .map(move |item| TurnTimelineItem {
                    turn_id: Some(turn_id.clone()),
                    item,
                })
        })
        .collect())
}

pub(super) fn read_entries(
    binding: &AgentBindingResolveRequest,
    ranges: Vec<TurnTimelineRange>,
    cancelled: &AtomicBool,
) -> ReadResult<Vec<(String, Value)>> {
    let mut decoded = Vec::new();
    for range in ranges {
        let error = |message: String| TurnTimelineReadError::InvalidRange {
            turn_id: range.turn_id.clone(),
            message,
        };
        let head = anchor(binding, &range.head_cursor).map_err(|e| error(e.to_string()))?;
        if head.is_none() && !range.is_first_session_turn {
            return Err(error(
                "source origin is only valid for the first Turn".into(),
            ));
        }
        let tail = range
            .tail_cursor
            .as_deref()
            .map(|c| anchor(binding, c))
            .transpose()
            .map_err(|e| error(e.to_string()))?;
        if tail.as_ref().is_some_and(Option::is_none) {
            return Err(error("terminal entry anchor missing".into()));
        }
        decoded.push(Range {
            id: range.turn_id,
            head,
            target: tail.flatten(),
            started: false,
            done: false,
            selected: true,
            usable: true,
            visited: HashSet::new(),
        });
    }
    Ok(scan(binding, decoded, false, cancelled)?
        .into_iter()
        .map(|(_, owner, entry)| (owner.expect("range entry has an owner"), entry))
        .collect())
}
