use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use pontia_agent_clients::get_client_spec;
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

const MAX_STREAMS: usize = 256;
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
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq)]
pub struct LiveOutputSnapshot {
    pub identity: LiveOutputIdentity,
    pub sequence: u64,
    pub items: Vec<LiveOutputItem>,
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

#[derive(Clone, Default)]
pub(crate) struct LiveOutputStore {
    inner: Arc<Mutex<HashMap<StreamKey, StreamState>>>,
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

        let mut streams = self.lock();
        prune_expired(&mut streams);
        let key = stream_key(&batch.producer.identity);
        let Some(existing) = streams.get(&key) else {
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

        let mut next = existing.clone();
        for (offset, update) in batch.updates.into_iter().enumerate() {
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
        streams.insert(key, next);
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

        let mut streams = self.lock();
        prune_expired(&mut streams);
        let key = stream_key(&replacement.producer.identity);
        if let Some(existing) = streams.get(&key)
            && replacement.sequence < existing.sequence
        {
            return Ok(LiveOutputPublishOutcome::Accepted {
                accepted_sequence: existing.sequence,
                duplicate: true,
            });
        }
        if !streams.contains_key(&key)
            && streams.iter().any(|(existing, state)| {
                existing.session_id == replacement.producer.identity.session_id
                    && existing.turn_id == replacement.producer.identity.turn_id
                    && !state.closed
            })
        {
            return Err(Error::StateConflict(format!(
                "turn {} already has an active live output stream",
                replacement.producer.identity.turn_id
            )));
        }
        if !streams.contains_key(&key) && streams.len() >= MAX_STREAMS {
            return Err(Error::StateConflict(format!(
                "live output stream capacity of {MAX_STREAMS} has been reached"
            )));
        }

        streams.insert(
            key,
            StreamState {
                sequence: replacement.sequence,
                items: replacement.items,
                accepted_updates: VecDeque::new(),
                closed: false,
                updated_at: Instant::now(),
            },
        );
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

        let mut streams = self.lock();
        prune_expired(&mut streams);
        let key = stream_key(&close.producer.identity);
        let Some(existing) = streams.get_mut(&key) else {
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
        Ok(LiveOutputPublishOutcome::Accepted {
            accepted_sequence: close.sequence,
            duplicate: false,
        })
    }

    pub fn snapshot(&self, session_id: &str, turn_id: &str) -> Option<LiveOutputSnapshot> {
        let mut streams = self.lock();
        prune_expired(&mut streams);
        let (key, state) = streams.iter().find(|(key, state)| {
            key.session_id == session_id && key.turn_id == turn_id && !state.closed
        })?;
        Some(LiveOutputSnapshot {
            identity: LiveOutputIdentity {
                session_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
                stream_id: key.stream_id.clone(),
            },
            sequence: state.sequence,
            items: state.items.clone(),
        })
    }

    pub fn discard_turn(&self, session_id: &str, turn_id: &str) {
        self.lock()
            .retain(|key, _| key.session_id != session_id || key.turn_id != turn_id);
    }

    pub fn discard_session(&self, session_id: &str) {
        self.lock().retain(|key, _| key.session_id != session_id);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<StreamKey, StreamState>> {
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
        } => {
            validate_non_empty("item_id", item_id)?;
            validate_non_empty("call_id", call_id)?;
            validate_non_empty("tool_name", tool_name)?;
            validate_tool_arguments(arguments)?;
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
                ..
            } => {
                validate_non_empty("call_id", call_id)?;
                validate_non_empty("tool_name", tool_name)?;
                let argument_bytes = validate_tool_arguments(arguments)?;
                total_tool_bytes = total_tool_bytes
                    .checked_add(argument_bytes)
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

fn validate_tool_arguments(arguments: &Value) -> Result<usize> {
    let size = serde_json::to_vec(arguments)?.len();
    if size > MAX_TOOL_CALL_BYTES {
        return Err(Error::Domain(format!(
            "tool call arguments exceed {MAX_TOOL_CALL_BYTES} bytes"
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
            });
        }
    }
    Ok(())
}

fn prune_expired(streams: &mut HashMap<StreamKey, StreamState>) {
    let now = Instant::now();
    streams.retain(|_, state| {
        now.duration_since(state.updated_at)
            < if state.closed {
                CLOSED_STREAM_TTL
            } else {
                ACTIVE_STREAM_TTL
            }
    });
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
        store.lock().insert(
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
        assert!(store.lock().is_empty());
    }
}
