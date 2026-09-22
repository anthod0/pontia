use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use pontia_agent_clients::{
    get_client_spec,
    raw_transcripts::{ManagedToolUse, ManagedToolUseInput},
};
use pontia_core::{
    domain::TurnState,
    error::{Error, Result},
};
use pontia_storage_sqlite::repositories::{
    runtime_bindings::SqliteRuntimeBindingRepository, sessions::SqliteSessionRepository,
    turns::SqliteTurnRepository,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;
use tokio::sync::broadcast;

const MAX_STREAMS: usize = 256;
const SUBSCRIBER_CAPACITY: usize = 64;
const MAX_ITEMS_PER_STREAM: usize = 256;
const MAX_UPDATES_PER_BATCH: usize = 256;
const MAX_TOTAL_TEXT_BYTES: usize = 1024 * 1024;
const MAX_TOTAL_TOOL_BYTES: usize = 1024 * 1024;
const MAX_TOOL_CALL_BYTES: usize = 64 * 1024;
const MAX_ID_BYTES: usize = 256;
const ACTIVE_STREAM_TTL: Duration = Duration::from_secs(30 * 60);
const CLOSED_STREAM_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LiveOutputItem {
    AssistantText {
        item_id: String,
        text: String,
    },
    ToolCall {
        item_id: String,
        call_id: String,
        tool_name: String,
        arguments: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        managed_tool_use: Option<ManagedToolUse>,
    },
}

impl LiveOutputItem {
    fn item_id(&self) -> &str {
        match self {
            Self::AssistantText { item_id, .. } | Self::ToolCall { item_id, .. } => item_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LiveOutputUpdate {
    AssistantTextDelta {
        item_id: String,
        delta: String,
    },
    ToolCall {
        item_id: String,
        call_id: String,
        tool_name: String,
        arguments: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        managed_tool_use: Option<ManagedToolUse>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveOutputIdentity {
    pub session_id: String,
    pub turn_id: String,
    pub stream_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveOutputProducer {
    pub identity: LiveOutputIdentity,
    pub runtime_instance_id: String,
}

#[derive(Debug, Clone)]
pub struct LiveOutputBatch {
    pub producer: LiveOutputProducer,
    pub first_sequence: u64,
    pub updates: Vec<LiveOutputUpdate>,
}

#[derive(Debug, Clone)]
pub struct LiveOutputSnapshotReplacement {
    pub producer: LiveOutputProducer,
    pub sequence: u64,
    pub items: Vec<LiveOutputItem>,
}

#[derive(Debug, Clone)]
pub struct LiveOutputClose {
    pub producer: LiveOutputProducer,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LiveOutputSnapshot {
    #[serde(flatten)]
    pub identity: LiveOutputIdentity,
    pub sequence: u64,
    pub items: Vec<LiveOutputItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveOutputCloseReason {
    ProducerClosed,
    Invalidated,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiveOutputStreamEvent {
    Snapshot {
        #[serde(flatten)]
        snapshot: LiveOutputSnapshot,
    },
    Updates {
        #[serde(flatten)]
        identity: LiveOutputIdentity,
        first_sequence: u64,
        updates: Vec<LiveOutputUpdate>,
    },
    Closed {
        #[serde(flatten)]
        identity: LiveOutputIdentity,
        sequence: u64,
        reason: LiveOutputCloseReason,
    },
}

pub struct LiveOutputSubscription {
    pub initial_snapshot: Option<LiveOutputSnapshot>,
    receiver: broadcast::Receiver<LiveOutputStreamEvent>,
}

impl LiveOutputSubscription {
    pub async fn recv(
        &mut self,
    ) -> std::result::Result<LiveOutputStreamEvent, broadcast::error::RecvError> {
        self.receiver.recv().await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveOutputPublishOutcome {
    Accepted {
        accepted_sequence: u64,
        duplicate: bool,
    },
    SnapshotRequired {
        accepted_sequence: u64,
    },
}

#[derive(Clone)]
pub(crate) struct LiveOutputStore {
    inner: Arc<Mutex<LiveOutputState>>,
}

struct LiveOutputState {
    streams: HashMap<StreamKey, StreamState>,
    sender: broadcast::Sender<LiveOutputStreamEvent>,
}

impl Default for LiveOutputStore {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(SUBSCRIBER_CAPACITY);
        Self {
            inner: Arc::new(Mutex::new(LiveOutputState {
                streams: HashMap::new(),
                sender,
            })),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct StreamKey {
    session_id: String,
    turn_id: String,
    stream_id: String,
}

#[derive(Clone)]
struct StreamState {
    sequence: u64,
    items: Vec<LiveOutputItem>,
    accepted_updates: VecDeque<(u64, LiveOutputUpdate)>,
    closed: bool,
    updated_at: Instant,
}

impl LiveOutputStore {
    pub fn publish_batch(&self, batch: LiveOutputBatch) -> Result<LiveOutputPublishOutcome> {
        validate_identity(&batch.producer.identity)?;
        if batch.updates.is_empty() {
            return Err(Error::Domain(
                "live output batch must contain at least one update".into(),
            ));
        }
        if batch.updates.len() > MAX_UPDATES_PER_BATCH {
            return Err(Error::Domain(format!(
                "live output batch exceeds {MAX_UPDATES_PER_BATCH} updates"
            )));
        }
        if batch.first_sequence == 0 {
            return Err(Error::Domain(
                "live output sequence must be positive".into(),
            ));
        }
        for update in &batch.updates {
            validate_update(update)?;
        }

        let mut state = self.lock();
        prune_expired(&mut state);
        let key = stream_key(&batch.producer.identity);
        let Some(existing) = state.streams.get(&key) else {
            return Ok(LiveOutputPublishOutcome::SnapshotRequired {
                accepted_sequence: 0,
            });
        };
        if existing.closed {
            return Ok(LiveOutputPublishOutcome::SnapshotRequired {
                accepted_sequence: existing.sequence,
            });
        }

        let update_count = u64::try_from(batch.updates.len())
            .map_err(|_| Error::Domain("live output batch is too large".into()))?;
        let last_sequence = batch
            .first_sequence
            .checked_add(update_count - 1)
            .ok_or_else(|| Error::Domain("live output sequence overflow".into()))?;
        if last_sequence <= existing.sequence {
            let exact_retry = batch.updates.iter().enumerate().all(|(offset, update)| {
                let sequence = batch.first_sequence + offset as u64;
                existing
                    .accepted_updates
                    .iter()
                    .any(|accepted| accepted.0 == sequence && accepted.1 == *update)
            });
            return Ok(if exact_retry {
                LiveOutputPublishOutcome::Accepted {
                    accepted_sequence: existing.sequence,
                    duplicate: true,
                }
            } else {
                LiveOutputPublishOutcome::SnapshotRequired {
                    accepted_sequence: existing.sequence,
                }
            });
        }
        if batch.first_sequence != existing.sequence + 1 {
            return Ok(LiveOutputPublishOutcome::SnapshotRequired {
                accepted_sequence: existing.sequence,
            });
        }

        let updates = batch.updates;
        let mut next = existing.clone();
        for (offset, update) in updates.iter().cloned().enumerate() {
            apply_update(&mut next.items, update.clone())?;
            next.accepted_updates
                .push_back((batch.first_sequence + offset as u64, update));
        }
        while next.accepted_updates.len() > MAX_UPDATES_PER_BATCH {
            next.accepted_updates.pop_front();
        }
        validate_items(&next.items)?;
        next.sequence = last_sequence;
        next.updated_at = Instant::now();
        state.streams.insert(key, next);
        let _ = state.sender.send(LiveOutputStreamEvent::Updates {
            identity: batch.producer.identity,
            first_sequence: batch.first_sequence,
            updates,
        });
        Ok(LiveOutputPublishOutcome::Accepted {
            accepted_sequence: last_sequence,
            duplicate: false,
        })
    }

    pub fn replace_snapshot(
        &self,
        replacement: LiveOutputSnapshotReplacement,
    ) -> Result<LiveOutputPublishOutcome> {
        validate_identity(&replacement.producer.identity)?;
        if replacement.sequence == 0 {
            return Err(Error::Domain(
                "live output sequence must be positive".into(),
            ));
        }
        validate_items(&replacement.items)?;

        let mut state = self.lock();
        prune_expired(&mut state);
        let key = stream_key(&replacement.producer.identity);
        if let Some(existing) = state.streams.get(&key)
            && (replacement.sequence < existing.sequence
                || (replacement.sequence == existing.sequence
                    && (existing.closed || replacement.items == existing.items)))
        {
            return Ok(LiveOutputPublishOutcome::Accepted {
                accepted_sequence: existing.sequence,
                duplicate: true,
            });
        }
        if !state.streams.contains_key(&key)
            && state.streams.iter().any(|(existing, stream)| {
                existing.session_id == replacement.producer.identity.session_id
                    && existing.turn_id == replacement.producer.identity.turn_id
                    && !stream.closed
            })
        {
            return Err(Error::StateConflict(format!(
                "turn {} already has an active live output stream",
                replacement.producer.identity.turn_id
            )));
        }
        if !state.streams.contains_key(&key) && state.streams.len() >= MAX_STREAMS {
            return Err(Error::StateConflict(format!(
                "live output stream capacity of {MAX_STREAMS} has been reached"
            )));
        }

        let snapshot = LiveOutputSnapshot {
            identity: replacement.producer.identity,
            sequence: replacement.sequence,
            items: replacement.items,
        };
        state.streams.insert(
            key,
            StreamState {
                sequence: snapshot.sequence,
                items: snapshot.items.clone(),
                accepted_updates: VecDeque::new(),
                closed: false,
                updated_at: Instant::now(),
            },
        );
        let _ = state
            .sender
            .send(LiveOutputStreamEvent::Snapshot { snapshot });
        Ok(LiveOutputPublishOutcome::Accepted {
            accepted_sequence: replacement.sequence,
            duplicate: false,
        })
    }

    pub fn close(&self, close: LiveOutputClose) -> Result<LiveOutputPublishOutcome> {
        validate_identity(&close.producer.identity)?;
        if close.sequence == 0 {
            return Err(Error::Domain(
                "live output sequence must be positive".into(),
            ));
        }

        let mut state = self.lock();
        prune_expired(&mut state);
        let key = stream_key(&close.producer.identity);
        let Some(existing) = state.streams.get_mut(&key) else {
            return Ok(LiveOutputPublishOutcome::SnapshotRequired {
                accepted_sequence: 0,
            });
        };
        if close.sequence <= existing.sequence {
            return Ok(LiveOutputPublishOutcome::Accepted {
                accepted_sequence: existing.sequence,
                duplicate: true,
            });
        }
        if close.sequence != existing.sequence + 1 {
            return Ok(LiveOutputPublishOutcome::SnapshotRequired {
                accepted_sequence: existing.sequence,
            });
        }

        existing.sequence = close.sequence;
        existing.items.clear();
        existing.accepted_updates.clear();
        existing.closed = true;
        existing.updated_at = Instant::now();
        let _ = state.sender.send(LiveOutputStreamEvent::Closed {
            identity: close.producer.identity,
            sequence: close.sequence,
            reason: LiveOutputCloseReason::ProducerClosed,
        });
        Ok(LiveOutputPublishOutcome::Accepted {
            accepted_sequence: close.sequence,
            duplicate: false,
        })
    }

    pub fn snapshot(&self, session_id: &str, turn_id: &str) -> Option<LiveOutputSnapshot> {
        let mut state = self.lock();
        prune_expired(&mut state);
        snapshot_for_turn(&state.streams, session_id, turn_id)
    }

    pub fn subscribe_session(&self, session_id: &str) -> LiveOutputSubscription {
        let mut state = self.lock();
        let receiver = state.sender.subscribe();
        prune_expired(&mut state);
        let initial_snapshot = snapshot_for_session(&state.streams, session_id);
        LiveOutputSubscription {
            initial_snapshot,
            receiver,
        }
    }

    pub fn discard_turn(&self, session_id: &str, turn_id: &str) {
        let mut state = self.lock();
        close_matching(
            &mut state,
            |key| key.session_id == session_id && key.turn_id == turn_id,
            LiveOutputCloseReason::Invalidated,
        );
    }

    pub fn discard_session(&self, session_id: &str) {
        let mut state = self.lock();
        close_matching(
            &mut state,
            |key| key.session_id == session_id,
            LiveOutputCloseReason::Invalidated,
        );
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, LiveOutputState> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[derive(Clone)]
pub struct LiveOutputService {
    pool: SqlitePool,
    store: LiveOutputStore,
}

impl LiveOutputService {
    pub(crate) fn new(pool: SqlitePool, store: LiveOutputStore) -> Self {
        Self { pool, store }
    }

    pub async fn publish_batch(&self, batch: LiveOutputBatch) -> Result<LiveOutputPublishOutcome> {
        self.validate_producer(&batch.producer, true).await?;
        self.store.publish_batch(batch)
    }

    pub async fn replace_snapshot(
        &self,
        replacement: LiveOutputSnapshotReplacement,
    ) -> Result<LiveOutputPublishOutcome> {
        self.validate_producer(&replacement.producer, true).await?;
        self.store.replace_snapshot(replacement)
    }

    pub async fn close(&self, close: LiveOutputClose) -> Result<LiveOutputPublishOutcome> {
        self.validate_producer(&close.producer, false).await?;
        self.store.close(close)
    }

    pub fn snapshot(&self, session_id: &str, turn_id: &str) -> Option<LiveOutputSnapshot> {
        self.store.snapshot(session_id, turn_id)
    }

    pub fn subscribe_session(&self, session_id: &str) -> LiveOutputSubscription {
        self.store.subscribe_session(session_id)
    }

    pub fn discard_turn(&self, session_id: &str, turn_id: &str) {
        self.store.discard_turn(session_id, turn_id);
    }

    pub fn discard_session(&self, session_id: &str) {
        self.store.discard_session(session_id);
    }

    async fn validate_producer(
        &self,
        producer: &LiveOutputProducer,
        require_active_turn: bool,
    ) -> Result<()> {
        let identity = &producer.identity;
        validate_identity(identity)?;
        validate_non_empty("runtime_instance_id", &producer.runtime_instance_id)?;

        let session = SqliteSessionRepository::new(self.pool.clone())
            .get_session(&identity.session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {} not found", identity.session_id)))?;
        let supports_streaming = get_client_spec(&session.client_type)
            .is_some_and(|spec| spec.capabilities.stream_output);
        if !supports_streaming {
            return Err(Error::CapabilityUnavailable(format!(
                "agent client {} does not support live output",
                session.client_type
            )));
        }

        let expected_runtime = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .runtime_instance_id(&identity.session_id)
            .await?;
        if expected_runtime.as_deref() != Some(producer.runtime_instance_id.as_str()) {
            return Err(Error::StateConflict(format!(
                "runtime_instance_id does not match session {} runtime binding",
                identity.session_id
            )));
        }

        let turn = SqliteTurnRepository::new(self.pool.clone())
            .get_projection(&identity.turn_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("turn {} not found", identity.turn_id)))?;
        if turn.session_id != identity.session_id {
            return Err(Error::StateConflict(format!(
                "turn {} belongs to session {}, not {}",
                identity.turn_id, turn.session_id, identity.session_id
            )));
        }
        let turn_state = turn.state.parse::<TurnState>()?;
        if require_active_turn && turn_state != TurnState::Running {
            return Err(Error::StateConflict(format!(
                "turn {} is {turn_state}, not running",
                identity.turn_id
            )));
        }
        Ok(())
    }
}

fn stream_key(identity: &LiveOutputIdentity) -> StreamKey {
    StreamKey {
        session_id: identity.session_id.clone(),
        turn_id: identity.turn_id.clone(),
        stream_id: identity.stream_id.clone(),
    }
}

fn validate_identity(identity: &LiveOutputIdentity) -> Result<()> {
    validate_non_empty("session_id", &identity.session_id)?;
    validate_non_empty("turn_id", &identity.turn_id)?;
    validate_non_empty("stream_id", &identity.stream_id)
}

fn validate_non_empty(field: &str, value: &str) -> Result<()> {
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

fn validate_update(update: &LiveOutputUpdate) -> Result<()> {
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

fn validate_items(items: &[LiveOutputItem]) -> Result<()> {
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

fn apply_update(items: &mut Vec<LiveOutputItem>, update: LiveOutputUpdate) -> Result<()> {
    match update {
        LiveOutputUpdate::AssistantTextDelta { item_id, delta } => {
            let extends_current = matches!(
                items.last(),
                Some(LiveOutputItem::AssistantText { item_id: current_id, .. }) if current_id == &item_id
            );
            if extends_current {
                let Some(LiveOutputItem::AssistantText { text, .. }) = items.last_mut() else {
                    unreachable!("checked current live output item")
                };
                text.push_str(&delta);
            } else if items.iter().any(|item| item.item_id() == item_id) {
                return Err(Error::StateConflict(format!(
                    "assistant text item {item_id} is not the current output item"
                )));
            } else {
                items.push(LiveOutputItem::AssistantText {
                    item_id,
                    text: delta,
                });
            }
        }
        LiveOutputUpdate::ToolCall {
            item_id,
            call_id,
            tool_name,
            arguments,
            managed_tool_use,
        } => {
            if items.iter().any(|item| item.item_id() == item_id) {
                return Err(Error::StateConflict(format!(
                    "live output item_id {item_id} already exists"
                )));
            }
            if items.iter().any(|item| {
                matches!(item, LiveOutputItem::ToolCall { call_id: existing, .. } if existing == &call_id)
            }) {
                return Err(Error::StateConflict(format!(
                    "live output call_id {call_id} already exists"
                )));
            }
            items.push(LiveOutputItem::ToolCall {
                item_id,
                call_id,
                tool_name,
                arguments,
                managed_tool_use,
            });
        }
    }
    Ok(())
}

fn snapshot_for_turn(
    streams: &HashMap<StreamKey, StreamState>,
    session_id: &str,
    turn_id: &str,
) -> Option<LiveOutputSnapshot> {
    let (key, state) = streams.iter().find(|(key, state)| {
        key.session_id == session_id && key.turn_id == turn_id && !state.closed
    })?;
    Some(snapshot_from_state(key, state))
}

fn snapshot_for_session(
    streams: &HashMap<StreamKey, StreamState>,
    session_id: &str,
) -> Option<LiveOutputSnapshot> {
    let (key, state) = streams
        .iter()
        .filter(|(key, state)| key.session_id == session_id && !state.closed)
        .max_by_key(|(_, state)| state.updated_at)?;
    Some(snapshot_from_state(key, state))
}

fn snapshot_from_state(key: &StreamKey, state: &StreamState) -> LiveOutputSnapshot {
    LiveOutputSnapshot {
        identity: LiveOutputIdentity {
            session_id: key.session_id.clone(),
            turn_id: key.turn_id.clone(),
            stream_id: key.stream_id.clone(),
        },
        sequence: state.sequence,
        items: state.items.clone(),
    }
}

fn close_matching(
    state: &mut LiveOutputState,
    matches: impl Fn(&StreamKey) -> bool,
    reason: LiveOutputCloseReason,
) {
    let closed = state
        .streams
        .iter()
        .filter(|(key, stream)| matches(key) && !stream.closed)
        .map(|(key, stream)| LiveOutputStreamEvent::Closed {
            identity: LiveOutputIdentity {
                session_id: key.session_id.clone(),
                turn_id: key.turn_id.clone(),
                stream_id: key.stream_id.clone(),
            },
            sequence: stream.sequence,
            reason,
        })
        .collect::<Vec<_>>();
    state.streams.retain(|key, _| !matches(key));
    for event in closed {
        let _ = state.sender.send(event);
    }
}

fn prune_expired(state: &mut LiveOutputState) {
    let now = Instant::now();
    let expired = state
        .streams
        .iter()
        .filter(|(_, stream)| {
            now.duration_since(stream.updated_at)
                >= if stream.closed {
                    CLOSED_STREAM_TTL
                } else {
                    ACTIVE_STREAM_TTL
                }
        })
        .map(|(key, _)| key.clone())
        .collect::<std::collections::HashSet<_>>();
    let expired_active = state
        .streams
        .iter()
        .filter(|(key, stream)| expired.contains(key) && !stream.closed)
        .map(|(key, _)| key.clone())
        .collect::<std::collections::HashSet<_>>();
    close_matching(
        state,
        |key| expired_active.contains(key),
        LiveOutputCloseReason::Expired,
    );
    state.streams.retain(|key, _| !expired.contains(key));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> LiveOutputIdentity {
        LiveOutputIdentity {
            session_id: "sess_1".into(),
            turn_id: "turn_1".into(),
            stream_id: "stream_1".into(),
        }
    }

    fn producer() -> LiveOutputProducer {
        LiveOutputProducer {
            identity: identity(),
            runtime_instance_id: "rtinst_1".into(),
        }
    }

    #[test]
    fn snapshot_then_batch_preserves_text_tool_text_order() {
        let store = LiveOutputStore::default();
        store
            .replace_snapshot(LiveOutputSnapshotReplacement {
                producer: producer(),
                sequence: 1,
                items: vec![LiveOutputItem::AssistantText {
                    item_id: "text_1".into(),
                    text: "hello".into(),
                }],
            })
            .unwrap();

        let outcome = store
            .publish_batch(LiveOutputBatch {
                producer: producer(),
                first_sequence: 2,
                updates: vec![
                    LiveOutputUpdate::ToolCall {
                        item_id: "tool_1".into(),
                        call_id: "call_1".into(),
                        tool_name: "read".into(),
                        arguments: serde_json::json!({"path": "README.md"}),
                        managed_tool_use: None,
                    },
                    LiveOutputUpdate::AssistantTextDelta {
                        item_id: "text_2".into(),
                        delta: "world".into(),
                    },
                ],
            })
            .unwrap();

        assert_eq!(
            outcome,
            LiveOutputPublishOutcome::Accepted {
                accepted_sequence: 3,
                duplicate: false,
            }
        );
        assert_eq!(
            store.snapshot("sess_1", "turn_1").unwrap().items,
            vec![
                LiveOutputItem::AssistantText {
                    item_id: "text_1".into(),
                    text: "hello".into(),
                },
                LiveOutputItem::ToolCall {
                    item_id: "tool_1".into(),
                    call_id: "call_1".into(),
                    tool_name: "read".into(),
                    arguments: serde_json::json!({"path": "README.md"}),
                    managed_tool_use: None,
                },
                LiveOutputItem::AssistantText {
                    item_id: "text_2".into(),
                    text: "world".into(),
                },
            ]
        );
    }

    #[test]
    fn close_is_idempotent_and_hides_the_snapshot() {
        let store = LiveOutputStore::default();
        store
            .replace_snapshot(LiveOutputSnapshotReplacement {
                producer: producer(),
                sequence: 1,
                items: vec![LiveOutputItem::AssistantText {
                    item_id: "text_1".into(),
                    text: "done".into(),
                }],
            })
            .unwrap();
        let close = LiveOutputClose {
            producer: producer(),
            sequence: 2,
        };

        assert_eq!(
            store.close(close.clone()).unwrap(),
            LiveOutputPublishOutcome::Accepted {
                accepted_sequence: 2,
                duplicate: false,
            }
        );
        assert!(store.snapshot("sess_1", "turn_1").is_none());
        assert_eq!(
            store.close(close).unwrap(),
            LiveOutputPublishOutcome::Accepted {
                accepted_sequence: 2,
                duplicate: true,
            }
        );
    }

    #[test]
    fn duplicate_batch_is_idempotent_and_gap_requests_snapshot() {
        let store = LiveOutputStore::default();
        store
            .replace_snapshot(LiveOutputSnapshotReplacement {
                producer: producer(),
                sequence: 1,
                items: vec![LiveOutputItem::AssistantText {
                    item_id: "text_1".into(),
                    text: "a".into(),
                }],
            })
            .unwrap();
        let batch = LiveOutputBatch {
            producer: producer(),
            first_sequence: 2,
            updates: vec![LiveOutputUpdate::AssistantTextDelta {
                item_id: "text_1".into(),
                delta: "b".into(),
            }],
        };
        store.publish_batch(batch.clone()).unwrap();
        assert_eq!(
            store.publish_batch(batch).unwrap(),
            LiveOutputPublishOutcome::Accepted {
                accepted_sequence: 2,
                duplicate: true,
            }
        );
        assert_eq!(
            store
                .publish_batch(LiveOutputBatch {
                    producer: producer(),
                    first_sequence: 2,
                    updates: vec![LiveOutputUpdate::AssistantTextDelta {
                        item_id: "text_1".into(),
                        delta: "conflict".into(),
                    }],
                })
                .unwrap(),
            LiveOutputPublishOutcome::SnapshotRequired {
                accepted_sequence: 2,
            }
        );
        assert_eq!(
            store
                .publish_batch(LiveOutputBatch {
                    producer: producer(),
                    first_sequence: 4,
                    updates: vec![LiveOutputUpdate::AssistantTextDelta {
                        item_id: "text_1".into(),
                        delta: "gap".into(),
                    }],
                })
                .unwrap(),
            LiveOutputPublishOutcome::SnapshotRequired {
                accepted_sequence: 2,
            }
        );
        assert_eq!(
            store.snapshot("sess_1", "turn_1").unwrap().items,
            vec![LiveOutputItem::AssistantText {
                item_id: "text_1".into(),
                text: "ab".into(),
            }]
        );
    }

    #[test]
    fn another_stream_cannot_replace_an_active_turn_stream() {
        let store = LiveOutputStore::default();
        store
            .replace_snapshot(LiveOutputSnapshotReplacement {
                producer: producer(),
                sequence: 1,
                items: Vec::new(),
            })
            .unwrap();
        let mut replacement_producer = producer();
        replacement_producer.identity.stream_id = "stream_2".into();

        let error = store
            .replace_snapshot(LiveOutputSnapshotReplacement {
                producer: replacement_producer,
                sequence: 1,
                items: Vec::new(),
            })
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("already has an active live output stream")
        );
        assert_eq!(
            store
                .snapshot("sess_1", "turn_1")
                .unwrap()
                .identity
                .stream_id,
            "stream_1"
        );
    }

    #[test]
    fn expired_streams_are_removed_lazily() {
        let store = LiveOutputStore::default();
        store.lock().streams.insert(
            stream_key(&identity()),
            StreamState {
                sequence: 1,
                items: Vec::new(),
                accepted_updates: VecDeque::new(),
                closed: false,
                updated_at: Instant::now() - ACTIVE_STREAM_TTL,
            },
        );

        assert!(store.snapshot("sess_1", "turn_1").is_none());
        assert!(store.lock().streams.is_empty());
    }

    #[tokio::test]
    async fn subscription_starts_with_an_atomic_snapshot_then_receives_updates() {
        let store = LiveOutputStore::default();
        store
            .replace_snapshot(LiveOutputSnapshotReplacement {
                producer: producer(),
                sequence: 1,
                items: vec![LiveOutputItem::AssistantText {
                    item_id: "text_1".into(),
                    text: "a".into(),
                }],
            })
            .unwrap();

        let mut subscription = store.subscribe_session("sess_1");
        assert_eq!(subscription.initial_snapshot.as_ref().unwrap().sequence, 1);
        store
            .publish_batch(LiveOutputBatch {
                producer: producer(),
                first_sequence: 2,
                updates: vec![LiveOutputUpdate::AssistantTextDelta {
                    item_id: "text_1".into(),
                    delta: "b".into(),
                }],
            })
            .unwrap();

        assert!(matches!(
            subscription.recv().await.unwrap(),
            LiveOutputStreamEvent::Updates {
                first_sequence: 2,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn retries_do_not_publish_and_session_discard_notifies_subscribers() {
        let store = LiveOutputStore::default();
        let replacement = LiveOutputSnapshotReplacement {
            producer: producer(),
            sequence: 1,
            items: Vec::new(),
        };
        store.replace_snapshot(replacement.clone()).unwrap();
        let mut subscription = store.subscribe_session("sess_1");
        assert!(subscription.initial_snapshot.take().is_some());

        assert_eq!(
            store.replace_snapshot(replacement).unwrap(),
            LiveOutputPublishOutcome::Accepted {
                accepted_sequence: 1,
                duplicate: true,
            }
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(10), subscription.recv())
                .await
                .is_err()
        );

        store.discard_session("sess_1");
        assert!(matches!(
            subscription.recv().await.unwrap(),
            LiveOutputStreamEvent::Closed { sequence: 1, .. }
        ));
    }

    #[tokio::test]
    async fn lagged_subscriber_can_recover_from_the_current_snapshot() {
        let store = LiveOutputStore::default();
        store
            .replace_snapshot(LiveOutputSnapshotReplacement {
                producer: producer(),
                sequence: 1,
                items: vec![LiveOutputItem::AssistantText {
                    item_id: "text_1".into(),
                    text: "a".into(),
                }],
            })
            .unwrap();
        let mut subscription = store.subscribe_session("sess_1");

        for sequence in 2..=(SUBSCRIBER_CAPACITY as u64 + 2) {
            store
                .publish_batch(LiveOutputBatch {
                    producer: producer(),
                    first_sequence: sequence,
                    updates: vec![LiveOutputUpdate::AssistantTextDelta {
                        item_id: "text_1".into(),
                        delta: "x".into(),
                    }],
                })
                .unwrap();
        }

        assert!(matches!(
            subscription.recv().await,
            Err(broadcast::error::RecvError::Lagged(_))
        ));
        assert_eq!(
            store
                .subscribe_session("sess_1")
                .initial_snapshot
                .unwrap()
                .sequence,
            SUBSCRIBER_CAPACITY as u64 + 2
        );
    }
}
