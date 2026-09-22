use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use pontia_core::error::{Error, Result};
use tokio::sync::broadcast;

use super::{
    LiveOutputBatch, LiveOutputClose, LiveOutputCloseReason, LiveOutputIdentity, LiveOutputItem,
    LiveOutputPublishOutcome, LiveOutputSnapshot, LiveOutputSnapshotReplacement,
    LiveOutputStreamEvent, LiveOutputUpdate,
    validation::{validate_identity, validate_items, validate_update},
};

const MAX_STREAMS: usize = 256;
const SUBSCRIBER_CAPACITY: usize = 64;
const MAX_UPDATES_PER_BATCH: usize = 256;
const ACTIVE_STREAM_TTL: Duration = Duration::from_secs(30 * 60);
const CLOSED_STREAM_TTL: Duration = Duration::from_secs(60);

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

fn stream_key(identity: &LiveOutputIdentity) -> StreamKey {
    StreamKey {
        session_id: identity.session_id.clone(),
        turn_id: identity.turn_id.clone(),
        stream_id: identity.stream_id.clone(),
    }
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
mod tests;
