//! Runtime history is independent of Pi's JSONL storage. Legacy dispatch lives separately.
mod recovery;

use crate::raw_transcripts::{PiAgentBindingResolver, PiJsonlV2Cursor, PiTimelineAdapter};
use pontia_application::{
    client_contract::{
        history::{RecoveredTurnHistory, TurnHistoryCandidate},
        native_history::{HistoryRead, NativeHistory},
        raw_transcripts::{
            AgentBindingResolveRequest, AgentBindingResolver, TurnTimelineItem, TurnTimelineRange,
            TurnTimelineReadError, TurnTimelineReadRequest, TurnTimelineReader,
        },
    },
    clients::ClientControlService,
};
use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::OnceCell;

const PREFIX: &str = "pi-entry-v1:";
const MAX_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PiEntryCursor {
    pub binding_id: String,
    pub session_id: String,
    pub anchor: Option<String>,
    pub relation: String,
}
impl PiEntryCursor {
    pub fn encode(&self) -> String {
        format!(
            "{PREFIX}{}",
            serde_json::to_string(self).expect("serializable entry cursor")
        )
    }
    pub fn decode(value: &str, binding: &str, session: Option<&str>) -> Result<Self> {
        let payload = value
            .strip_prefix(PREFIX)
            .ok_or_else(|| invalid("unknown locator generation"))?;
        let cursor: Self =
            serde_json::from_str(payload).map_err(|_| invalid("malformed entry locator"))?;
        if cursor.binding_id != binding
            || !identity(&cursor.session_id)
            || session.is_some_and(|s| s != cursor.session_id)
            || cursor.relation != "after"
            || cursor.anchor.as_deref().is_some_and(|a| !identity(a))
            || !serde_json::from_str::<Value>(payload)?
                .as_object()
                .is_some_and(|o| o.contains_key("anchor"))
        {
            return Err(invalid("entry locator identity or relation mismatch"));
        }
        Ok(cursor)
    }
    pub fn is_entry(value: &str) -> bool {
        value.starts_with(PREFIX)
    }
}
fn identity(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 512
}
fn invalid(message: &str) -> Error {
    Error::Domain(format!("range_invalid: {message}"))
}
fn unavailable(message: impl std::fmt::Display) -> Error {
    Error::CapabilityUnavailable(format!("source_unavailable: {message}"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    session_id: String,
    snapshot: String,
    entry_count: usize,
    upper_entry_id: Option<String>,
    leaf_id: Option<String>,
    entries: Vec<Value>,
    continuation: Option<String>,
}
struct RuntimeSnapshot {
    connection: pontia_application::clients::NativeHistoryConnection,
    entries: Vec<Value>,
    by_id: HashMap<String, usize>,
    leaf: Option<String>,
}
impl RuntimeSnapshot {
    async fn load(control: &ClientControlService, session: &str, native: &str) -> Result<Self> {
        let connection = control.history_source(session).await?;
        let mut params = json!({"page_size":128});
        let mut snapshot = None;
        let mut entry_count = None;
        let mut upper = None;
        let mut leaf = None;
        let mut entries = Vec::new();
        let mut by_id = HashMap::new();
        let mut continuations = HashSet::new();
        let mut bytes = 0usize;
        loop {
            let response = connection.read(params).await?;
            if serde_json::to_vec(&response)?.len() > 1024 * 1024 {
                return Err(unavailable("history page exceeds response budget"));
            }
            if !response.as_object().is_some_and(|o| {
                [
                    "session_id",
                    "snapshot",
                    "entry_count",
                    "upper_entry_id",
                    "leaf_id",
                    "entries",
                    "continuation",
                ]
                .iter()
                .all(|key| o.contains_key(*key))
            }) {
                return Err(unavailable("invalid history response structure"));
            }
            let page: Page = serde_json::from_value(response)
                .map_err(|_| unavailable("invalid history response"))?;
            if page.session_id != native
                || page.snapshot.is_empty()
                || page.snapshot.len() > 4096
                || page.entries.len() > 128
                || page.entry_count > MAX_ENTRIES
                || page
                    .upper_entry_id
                    .as_deref()
                    .is_some_and(|id| !identity(id))
                || page.leaf_id.as_deref().is_some_and(|id| !identity(id))
            {
                return Err(unavailable("history response identity mismatch"));
            }
            if let Some(previous) = &snapshot {
                if previous != &page.snapshot
                    || upper != page.upper_entry_id
                    || leaf != page.leaf_id
                    || entry_count != Some(page.entry_count)
                {
                    return Err(unavailable("history snapshot changed during pagination"));
                }
            } else {
                snapshot = Some(page.snapshot.clone());
                entry_count = Some(page.entry_count);
                upper = page.upper_entry_id.clone();
                leaf = page.leaf_id.clone();
            }
            if page.entries.is_empty() && (page.continuation.is_some() || !entries.is_empty()) {
                return Err(unavailable("empty history continuation page"));
            }
            for entry in page.entries {
                let id = entry["id"]
                    .as_str()
                    .filter(|s| identity(s))
                    .ok_or_else(|| unavailable("missing entry identity"))?;
                if !entry["type"].as_str().is_some_and(identity)
                    || !entry["timestamp"].as_str().is_some_and(identity)
                    || !entry
                        .as_object()
                        .is_some_and(|o| o.contains_key("parentId"))
                {
                    return Err(unavailable("invalid history entry fields"));
                }
                match &entry["parentId"] {
                    Value::Null => {}
                    Value::String(parent) if by_id.contains_key(parent) => {}
                    _ => return Err(unavailable("invalid history parent or append order")),
                }
                if by_id.insert(id.to_owned(), entries.len()).is_some() {
                    return Err(unavailable("duplicate history entry identity"));
                }
                bytes = bytes
                    .checked_add(serde_json::to_vec(&entry)?.len())
                    .ok_or_else(|| unavailable("history exceeds memory budget"))?;
                if bytes > MAX_SNAPSHOT_BYTES || entries.len() >= page.entry_count {
                    return Err(unavailable("history exceeds snapshot budget"));
                }
                entries.push(entry);
            }
            match page.continuation {
                Some(next) => {
                    if next.is_empty()
                        || next.len() > 4096
                        || !continuations.insert(next.clone())
                        || entries.last().and_then(|e| e["id"].as_str()) == upper.as_deref()
                    {
                        return Err(unavailable("invalid history continuation"));
                    }
                    params = json!({"page_size":128,"snapshot":page.snapshot,"continuation":next});
                }
                None => break,
            }
        }
        if Some(entries.len()) != entry_count
            || entries.last().and_then(|e| e["id"].as_str()) != upper.as_deref()
            || leaf.as_ref().is_some_and(|id| !by_id.contains_key(id))
        {
            return Err(unavailable("invalid history upper bound or leaf"));
        }
        connection.validate().await?;
        Ok(Self {
            connection,
            entries,
            by_id,
            leaf,
        })
    }
    fn chain(&self, head: Option<&str>, tail: Option<&str>) -> Result<Vec<&Value>> {
        if head.is_some_and(|id| !self.by_id.contains_key(id))
            || tail.is_some_and(|id| !self.by_id.contains_key(id))
        {
            return Err(invalid("entry anchor missing from snapshot"));
        }
        let mut current = tail;
        let mut selected = Vec::new();
        while current != head {
            let id = current.ok_or_else(|| invalid("head is not an ancestor of tail"))?;
            let entry = &self.entries[*self
                .by_id
                .get(id)
                .ok_or_else(|| invalid("unknown parent"))?];
            selected.push(entry);
            current = entry["parentId"].as_str();
        }
        selected.reverse();
        Ok(selected)
    }
}

pub(crate) struct PiHistory {
    binding: AgentBindingResolveRequest,
    control: Option<ClientControlService>,
    snapshot: OnceCell<std::result::Result<RuntimeSnapshot, String>>,
}
impl PiHistory {
    pub(crate) fn new(
        binding: AgentBindingResolveRequest,
        control: Option<ClientControlService>,
    ) -> Arc<Self> {
        Arc::new(Self {
            binding,
            control,
            snapshot: OnceCell::new(),
        })
    }
    async fn snapshot(&self) -> Result<&RuntimeSnapshot> {
        let snapshot = self
            .snapshot
            .get_or_init(|| async {
                let Some(control) = &self.control else {
                    return Err("no runtime history router".into());
                };
                RuntimeSnapshot::load(
                    control,
                    &self.binding.session_id,
                    &self.binding.client_session_key,
                )
                .await
                .map_err(|error| error.to_string())
            })
            .await
            .as_ref()
            .map_err(unavailable)?;
        snapshot.connection.validate().await?;
        Ok(snapshot)
    }
    fn turn_generation(&self, head: Option<&str>, tail: Option<&str>) -> Result<bool> {
        let decode = |cursor: &str| -> Result<bool> {
            if PiEntryCursor::is_entry(cursor) {
                PiEntryCursor::decode(
                    cursor,
                    &self.binding.id,
                    Some(&self.binding.client_session_key),
                )?;
                Ok(true)
            } else {
                PiJsonlV2Cursor::decode(cursor, &self.binding.id)?;
                Ok(false)
            }
        };
        let head = head.map(decode).transpose()?;
        let tail = tail.map(decode).transpose()?;
        if head.zip(tail).is_some_and(|(h, t)| h != t) {
            return Err(invalid("mixed locator generations"));
        }
        Ok(head.or(tail).unwrap_or(true))
    }
    fn generation(&self, range: &TurnTimelineRange) -> Result<bool> {
        self.turn_generation(Some(&range.head_cursor), range.tail_cursor.as_deref())
    }
    fn selected<'a>(
        &self,
        snapshot: &'a RuntimeSnapshot,
        range: &TurnTimelineRange,
    ) -> Result<Vec<&'a Value>> {
        let head = PiEntryCursor::decode(
            &range.head_cursor,
            &self.binding.id,
            Some(&self.binding.client_session_key),
        )?;
        if head.anchor.is_none() && !range.is_first_session_turn {
            return Err(invalid("source origin is only valid for the first Turn"));
        }
        let tail = range
            .tail_cursor
            .as_deref()
            .map(|c| {
                PiEntryCursor::decode(c, &self.binding.id, Some(&self.binding.client_session_key))
            })
            .transpose()?;
        if tail.as_ref().is_some_and(|c| c.anchor.is_none()) {
            return Err(invalid("terminal entry anchor missing"));
        }
        snapshot.chain(
            head.anchor.as_deref(),
            tail.as_ref()
                .map_or(snapshot.leaf.as_deref(), |c| c.anchor.as_deref()),
        )
    }
    fn cursor(&self, anchor: Option<String>) -> String {
        PiEntryCursor {
            binding_id: self.binding.id.clone(),
            session_id: self.binding.client_session_key.clone(),
            anchor,
            relation: "after".into(),
        }
        .encode()
    }
}
impl NativeHistory for PiHistory {
    fn read_ranges(
        &self,
        ranges: Vec<TurnTimelineRange>,
    ) -> HistoryRead<'_, std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError>> {
        Box::pin(async move {
            let mut legacy = Vec::new();
            let mut runtime = Vec::new();
            for range in ranges {
                let entry = self.generation(&range).map_err(|error| {
                    TurnTimelineReadError::InvalidRange {
                        turn_id: range.turn_id.clone(),
                        message: error.to_string(),
                    }
                })?;
                if entry {
                    runtime.push(range);
                } else {
                    legacy.push(range);
                }
            }
            let mut result = Vec::new();
            if !legacy.is_empty() {
                let source = PiAgentBindingResolver::new().resolve(&self.binding)?;
                result.extend(PiTimelineAdapter::new().read_turn_ranges(
                    TurnTimelineReadRequest {
                        source,
                        ranges: legacy,
                    },
                )?);
            }
            if !runtime.is_empty() {
                let snapshot = self.snapshot().await?;
                let mut claimed = HashSet::new();
                for range in runtime {
                    let entries = self.selected(snapshot, &range).map_err(|error| {
                        TurnTimelineReadError::InvalidRange {
                            turn_id: range.turn_id.clone(),
                            message: error.to_string(),
                        }
                    })?;
                    for entry in entries {
                        if !claimed.insert(entry["id"].as_str().unwrap()) {
                            return Err(TurnTimelineReadError::InvalidRange {
                                turn_id: range.turn_id,
                                message: "overlapping Turn ranges".into(),
                            });
                        }
                        result.extend(
                            crate::raw_transcripts::mapping::pi_entry_to_items(entry, 0)
                                .into_iter()
                                .map(|item| TurnTimelineItem {
                                    turn_id: range.turn_id.clone(),
                                    item,
                                }),
                        );
                    }
                }
                snapshot.connection.validate().await?;
            }
            result.sort_by(|a, b| a.turn_id.cmp(&b.turn_id));
            Ok(result)
        })
    }
    fn branch_target(&self, range: TurnTimelineRange) -> HistoryRead<'_, Result<String>> {
        Box::pin(async move {
            if range.tail_cursor.is_none() {
                return Err(invalid("branch target tail missing"));
            }
            if !self.generation(&range)? {
                let source = PiAgentBindingResolver::new()
                    .resolve(&self.binding)
                    .map_err(|_| unavailable("legacy history source unavailable"))?;
                use crate::raw_transcripts::PiTurnUserEntryResolver;
                return PiTimelineAdapter::new()
                    .resolve_user_entry(crate::raw_transcripts::PiTurnUserEntryResolveRequest {
                        source,
                        session_id: self.binding.session_id.clone(),
                        turn_session_id: self.binding.session_id.clone(),
                        turn_id: range.turn_id,
                        is_first_session_turn: range.is_first_session_turn,
                        head_cursor: Some(range.head_cursor),
                        tail_cursor: range.tail_cursor,
                    })
                    .map(|e| e.entry_id)
                    .map_err(|e| invalid(&e.to_string()));
            }
            let snapshot = self.snapshot().await?;
            let selected = self.selected(snapshot, &range)?;
            let mut users = selected
                .iter()
                .filter(|e| e["type"] == "message" && e["message"]["role"] == "user");
            let user = users
                .next()
                .ok_or_else(|| invalid("branch user entry missing"))?;
            if users.next().is_some() {
                return Err(invalid("branch user entry ambiguous"));
            }
            Ok(user["id"].as_str().unwrap().into())
        })
    }
    fn recover(
        &self,
        turns: Vec<TurnHistoryCandidate>,
    ) -> HistoryRead<'_, Result<Vec<RecoveredTurnHistory>>> {
        Box::pin(async move { self.recover_turns(turns).await })
    }
}
