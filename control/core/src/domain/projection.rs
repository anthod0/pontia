use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{DomainEvent, EventType, SessionState, TimelineBoundary, TurnState, TurnTopology};
use crate::error::Error;

pub const MAX_TURN_INPUT_SUMMARY_CHARS: usize = 1_000;
pub const MAX_TURN_OUTPUT_SUMMARY_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProjection {
    pub session_id: String,
    pub client_type: String,
    pub title: Option<String>,
    pub handle: Option<String>,
    pub role: Option<String>,
    pub description: Option<String>,
    pub execution_profile_id: Option<String>,
    pub execution_profile_version: Option<String>,
    pub state: SessionState,
    pub current_turn_id: Option<String>,
    pub state_version: i64,
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnProjection {
    pub turn_id: String,
    pub session_id: String,
    pub head_cursor: Option<String>,
    pub tail_cursor: Option<String>,
    pub topology: TurnTopology,
    pub state: TurnState,
    pub state_version: i64,
    pub input_summary: Option<String>,
    pub output_summary: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Default, Clone)]
pub struct ProjectionState {
    sessions: HashMap<String, SessionProjection>,
    turns: HashMap<String, TurnProjection>,
    runtime_bindings: HashMap<String, String>,
}

impl ProjectionState {
    pub fn with_existing(
        sessions: impl IntoIterator<Item = SessionProjection>,
        turns: impl IntoIterator<Item = TurnProjection>,
    ) -> Self {
        Self {
            sessions: sessions
                .into_iter()
                .map(|s| (s.session_id.clone(), s))
                .collect(),
            turns: turns.into_iter().map(|t| (t.turn_id.clone(), t)).collect(),
            runtime_bindings: HashMap::new(),
        }
    }

    pub fn session(&self, session_id: &str) -> Option<&SessionProjection> {
        self.sessions.get(session_id)
    }

    pub fn turn(&self, turn_id: &str) -> Option<&TurnProjection> {
        self.turns.get(turn_id)
    }

    pub fn sessions(&self) -> impl Iterator<Item = &SessionProjection> {
        self.sessions.values()
    }

    pub fn turns(&self) -> impl Iterator<Item = &TurnProjection> {
        self.turns.values()
    }

    pub fn record_runtime_binding(&mut self, session_id: &str, binding: &str) {
        self.runtime_bindings
            .insert(session_id.to_string(), binding.to_string());
    }

    pub fn apply(&mut self, event: &DomainEvent) -> crate::error::Result<()> {
        if event.event_type.requires_turn_id() && event.turn_id.is_none() {
            return Err(Error::Domain(format!(
                "event {} requires turn_id",
                event.event_type
            )));
        }
        match (&event.timeline_boundary, event.event_type) {
            (None, _) | (Some(TimelineBoundary::Head { .. }), EventType::TurnStarted) => {}
            (
                Some(TimelineBoundary::Tail { .. }),
                EventType::TurnCompleted
                | EventType::TurnFailed
                | EventType::TurnDispatchFailed
                | EventType::TurnAbandoned
                | EventType::TurnInterrupted
                | EventType::TurnCancelled,
            ) => {}
            _ => {
                return Err(Error::Domain(format!(
                    "event {} cannot carry its timeline boundary position",
                    event.event_type
                )));
            }
        }
        if event.topology.is_some() && event.event_type != EventType::TurnStarted {
            return Err(Error::Domain(format!(
                "event {} cannot carry Turn topology enrichment",
                event.event_type
            )));
        }

        if let Some(session) = self.sessions.get(&event.session_id)
            && session.state.is_terminal()
            && !(session.state == SessionState::Exited
                && event.event_type == EventType::SessionResuming)
            && event.event_type != EventType::SessionTitleUpdated
            && event.event_type != EventType::SessionContextUsageUpdated
        {
            if event.topology.is_some() {
                self.apply_topology_to_existing_turn(event)?;
            }
            return Ok(());
        }

        match event.event_type {
            EventType::SessionCreated => self.apply_session(event, SessionState::Created),
            EventType::SessionStarting | EventType::SessionResuming => {
                self.apply_session(event, SessionState::Starting)
            }
            EventType::SessionStarted => self.apply_session(event, SessionState::Starting),
            EventType::SessionReady => self.apply_session(event, SessionState::Idle),
            EventType::SessionExited => {
                self.abandon_active_turn_for_session_exit(event)?;
                self.apply_session(event, SessionState::Exited)
            }
            EventType::SessionError => self.apply_session(event, SessionState::Error),
            EventType::SessionTitleUpdated => self.apply_session(
                event,
                self.sessions
                    .get(&event.session_id)
                    .map(|session| session.state)
                    .unwrap_or(SessionState::Created),
            ),
            EventType::SessionMessageUpdated => Ok(()),
            EventType::SessionContextUsageUpdated => self.apply_context_usage(event),
            EventType::TurnCreated | EventType::TurnQueued => {
                self.apply_turn(event, TurnState::Queued)
            }
            EventType::TurnStarted | EventType::TurnOutput | EventType::TurnInterruptRequested => {
                self.apply_turn(event, TurnState::Running)
            }
            EventType::TurnCompleted => self.apply_turn(event, TurnState::Completed),
            EventType::TurnFailed | EventType::TurnDispatchFailed | EventType::TurnAbandoned => {
                self.apply_turn(event, TurnState::Failed)
            }
            EventType::TurnInterrupted => self.apply_turn(event, TurnState::Interrupted),
            EventType::TurnCancelled => self.apply_turn(event, TurnState::Cancelled),
            EventType::InboxMessageQueued
            | EventType::InboxMessageDispatched
            | EventType::InboxMessageCancelled
            | EventType::InboxMessageSuperseded
            | EventType::InboxMessageFailed
            | EventType::InboxMessageDismissed => Ok(()),
        }
    }

    fn apply_session(
        &mut self,
        event: &DomainEvent,
        state: SessionState,
    ) -> crate::error::Result<()> {
        let session = self
            .sessions
            .entry(event.session_id.clone())
            .or_insert_with(|| SessionProjection {
                session_id: event.session_id.clone(),
                client_type: event.client_type.clone(),
                title: None,
                handle: None,
                role: None,
                description: None,
                execution_profile_id: None,
                execution_profile_version: None,
                state: SessionState::Created,
                current_turn_id: None,
                state_version: 0,
                metadata: Value::Object(Default::default()),
            });

        session.state = state;
        if event.event_type == EventType::SessionCreated {
            session.title = event
                .payload
                .get("title")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            session.handle = event
                .payload
                .get("handle")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            session.role = event
                .payload
                .get("role")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            session.description = event
                .payload
                .get("description")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            session.execution_profile_id = event
                .payload
                .get("execution_profile_id")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            session.execution_profile_version = event
                .payload
                .get("execution_profile_version")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            if let Some(metadata) = event.payload.get("metadata") {
                session.metadata = metadata.clone();
            }
        }
        if event.event_type == EventType::SessionTitleUpdated {
            session.title = event
                .payload
                .get("title")
                .and_then(Value::as_str)
                .map(ToString::to_string);
        }
        session.state_version += 1;
        Ok(())
    }

    fn abandon_active_turn_for_session_exit(
        &mut self,
        event: &DomainEvent,
    ) -> crate::error::Result<()> {
        let Some(turn_id) = self.active_turn_id(&event.session_id)?.map(str::to_string) else {
            return Ok(());
        };
        let Some(turn) = self.turns.get_mut(&turn_id) else {
            return Ok(());
        };

        turn.state = TurnState::Abandoned;
        turn.state_version += 1;
        if !turn.metadata.is_object() {
            turn.metadata = json!({});
        }
        if let Some(metadata) = turn.metadata.as_object_mut() {
            metadata.insert(
                "terminal_provenance".to_string(),
                json!({
                    "event_id": event.event_id,
                    "event_type": event.event_type.to_string(),
                    "reason": "session_exited_without_terminal_fact",
                    "source": "pontia_projection",
                }),
            );
        }
        Ok(())
    }

    fn active_turn_id(&self, session_id: &str) -> crate::error::Result<Option<&str>> {
        let mut active_turns = self
            .turns
            .values()
            .filter(|turn| turn.session_id == session_id && turn.state.is_active());
        let active_turn_id = active_turns.next().map(|turn| turn.turn_id.as_str());
        if active_turns.next().is_some() {
            return Err(Error::Domain(format!(
                "session {session_id} has multiple active Turns"
            )));
        }
        Ok(active_turn_id)
    }

    fn apply_context_usage(&mut self, event: &DomainEvent) -> crate::error::Result<()> {
        let Some(session) = self.sessions.get_mut(&event.session_id) else {
            return Ok(());
        };
        let usage = event
            .payload
            .get("context_usage")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                Error::Domain("payload.context_usage must be a JSON object".to_string())
            })?;
        let observed_at = event
            .occurred_at
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|err| Error::Domain(format!("invalid event timestamp: {err}")))?;
        let has_observed_usage = [
            "used_tokens",
            "max_tokens",
            "remaining_tokens",
            "usage_ratio",
            "input_tokens",
            "output_tokens",
            "cache_tokens",
        ]
        .iter()
        .any(|field| usage.get(*field).is_some_and(|value| !value.is_null()));

        if !session.metadata.is_object() {
            session.metadata = json!({});
        }
        if let Some(metadata) = session.metadata.as_object_mut() {
            let existing = metadata
                .get("context_usage")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            if has_observed_usage || !existing.is_empty() {
                let mut context_usage = serde_json::Map::new();
                for field in [
                    "used_tokens",
                    "max_tokens",
                    "remaining_tokens",
                    "usage_ratio",
                    "input_tokens",
                    "output_tokens",
                    "cache_tokens",
                ] {
                    let value = usage
                        .get(field)
                        .filter(|value| !value.is_null())
                        .cloned()
                        .or_else(|| existing.get(field).cloned())
                        .unwrap_or(Value::Null);
                    context_usage.insert(field.to_string(), value);
                }
                let confidence = usage
                    .get("confidence")
                    .filter(|value| !value.is_null())
                    .cloned()
                    .or_else(|| existing.get("confidence").cloned())
                    .unwrap_or_else(|| json!("unknown"));
                context_usage.insert("confidence".to_string(), confidence);
                context_usage.insert("observed_at".to_string(), json!(observed_at));
                metadata.insert("context_usage".to_string(), Value::Object(context_usage));
            }
            if let Some(model) = event.payload.get("model").filter(|model| !model.is_null()) {
                metadata.insert("model".to_string(), model.clone());
            }
        }
        session.state_version += 1;
        Ok(())
    }

    fn apply_turn(
        &mut self,
        event: &DomainEvent,
        new_state: TurnState,
    ) -> crate::error::Result<()> {
        let turn_id = event.turn_id.as_deref().expect("validated turn_id");

        self.validate_topology(event, turn_id)?;

        if let Some(existing) = self.turns.get(turn_id) {
            if existing.session_id != event.session_id {
                return Err(Error::Domain(format!(
                    "turn {turn_id} identity does not match immutable session_id"
                )));
            }
            if existing.state.is_terminal() {
                if let Some(turn) = self.turns.get_mut(turn_id) {
                    apply_resolved_topology(turn, event.topology.as_ref());
                }
                return Ok(());
            }
        }

        if new_state.is_active()
            && let Some(active_turn_id) = self.active_turn_id(&event.session_id)?
            && active_turn_id != turn_id
        {
            return Err(Error::Domain(format!(
                "session {} already has active turn {}",
                event.session_id, active_turn_id
            )));
        }

        let turn = self
            .turns
            .entry(turn_id.to_string())
            .or_insert_with(|| TurnProjection {
                turn_id: turn_id.to_string(),
                session_id: event.session_id.clone(),
                head_cursor: None,
                tail_cursor: None,
                topology: TurnTopology::Unknown,
                state: TurnState::Queued,
                state_version: 0,
                input_summary: None,
                output_summary: None,
                metadata: Value::Object(Default::default()),
            });

        match &event.timeline_boundary {
            Some(TimelineBoundary::Head { cursor }) => {
                turn.head_cursor = Some(cursor.clone());
            }
            Some(TimelineBoundary::Tail { cursor }) => {
                turn.tail_cursor = Some(cursor.clone());
            }
            None => {}
        }
        apply_resolved_topology(turn, event.topology.as_ref());

        turn.state = new_state;
        if matches!(
            event.event_type,
            EventType::TurnCreated | EventType::TurnQueued | EventType::TurnStarted
        ) && turn.input_summary.is_none()
            && let Some(summary) = summary_from_payload(&event.payload, "input", "input_summary")
        {
            turn.input_summary = Some(truncate_chars(summary, MAX_TURN_INPUT_SUMMARY_CHARS));
        }
        if matches!(
            event.event_type,
            EventType::TurnOutput | EventType::TurnCompleted
        ) && turn.output_summary.is_none()
            && let Some(summary) = summary_from_payload(&event.payload, "output", "output_summary")
        {
            turn.output_summary = Some(truncate_chars(summary, MAX_TURN_OUTPUT_SUMMARY_CHARS));
        }
        if event.event_type == EventType::TurnCreated
            && let Some(metadata) = event.payload.get("metadata")
        {
            turn.metadata = metadata.clone();
        }
        if event.event_type == EventType::TurnStarted
            && let Some(metadata) = event.payload.get("metadata").and_then(Value::as_object)
        {
            if !turn.metadata.is_object() {
                turn.metadata = json!({});
            }
            if let Some(turn_metadata) = turn.metadata.as_object_mut() {
                for (key, value) in metadata {
                    turn_metadata.insert(key.clone(), value.clone());
                }
            }
        }
        turn.state_version += 1;

        let session = self
            .sessions
            .entry(event.session_id.clone())
            .or_insert_with(|| SessionProjection {
                session_id: event.session_id.clone(),
                client_type: event.client_type.clone(),
                title: None,
                handle: None,
                role: None,
                description: None,
                execution_profile_id: None,
                execution_profile_version: None,
                state: SessionState::Created,
                current_turn_id: None,
                state_version: 0,
                metadata: Value::Object(Default::default()),
            });

        if session
            .title
            .as_ref()
            .is_none_or(|title| title.trim().is_empty())
            && let Some(title) = title_from_turn_input(&event.payload)
        {
            session.title = Some(title);
        }

        match new_state {
            TurnState::Queued => {}
            TurnState::Running => {
                if event.event_type == EventType::TurnStarted {
                    session.current_turn_id = Some(turn_id.to_string());
                }
                session.state = SessionState::Busy;
                session.state_version += 1;
            }
            TurnState::Completed
            | TurnState::Failed
            | TurnState::Cancelled
            | TurnState::Abandoned => {
                if session.state == SessionState::Busy {
                    session.state = SessionState::Idle;
                    session.state_version += 1;
                }
            }
            TurnState::Interrupted => {
                session.state = SessionState::Interrupted;
                session.state_version += 1;
            }
        }

        Ok(())
    }

    fn validate_topology(&self, event: &DomainEvent, turn_id: &str) -> crate::error::Result<()> {
        let Some(topology) = &event.topology else {
            return Ok(());
        };

        if let Some(existing) = self.turns.get(turn_id) {
            if existing.session_id != event.session_id {
                return Err(Error::Domain(format!(
                    "turn {turn_id} identity does not match immutable session_id"
                )));
            }
            if existing.topology != TurnTopology::Unknown
                && *topology != TurnTopology::Unknown
                && existing.topology != *topology
            {
                return Err(Error::Domain(format!(
                    "turn {turn_id} topology is already resolved as {}",
                    existing.topology.status()
                )));
            }
        }

        let TurnTopology::Linked { parent_turn_id } = topology else {
            return Ok(());
        };
        let parent = self.turns.get(parent_turn_id).ok_or_else(|| {
            Error::Domain(format!(
                "linked parent {parent_turn_id} does not identify an earlier Turn"
            ))
        })?;
        if parent.session_id != event.session_id {
            return Err(Error::Domain(format!(
                "linked parent {parent_turn_id} belongs to a different Session"
            )));
        }
        if parent.turn_id.as_str() >= turn_id {
            return Err(Error::Domain(format!(
                "linked parent {parent_turn_id} must precede turn {turn_id}"
            )));
        }
        Ok(())
    }

    fn apply_topology_to_existing_turn(&mut self, event: &DomainEvent) -> crate::error::Result<()> {
        let turn_id = event.turn_id.as_deref().expect("validated turn_id");
        if !self.turns.contains_key(turn_id) {
            return Err(Error::Domain(format!(
                "Turn topology enrichment cannot create missing turn {turn_id}"
            )));
        }
        self.validate_topology(event, turn_id)?;
        apply_resolved_topology(
            self.turns
                .get_mut(turn_id)
                .expect("validated existing Turn"),
            event.topology.as_ref(),
        );
        Ok(())
    }
}

fn apply_resolved_topology(turn: &mut TurnProjection, topology: Option<&TurnTopology>) {
    if let Some(topology) = topology
        && *topology != TurnTopology::Unknown
    {
        turn.topology = topology.clone();
    }
}

fn summary_from_payload<'a>(
    payload: &'a Value,
    nested_key: &str,
    legacy_key: &str,
) -> Option<&'a str> {
    payload
        .get(nested_key)
        .and_then(|value| value.get("summary"))
        .or_else(|| payload.get(legacy_key))
        .and_then(Value::as_str)
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn title_from_turn_input(payload: &Value) -> Option<String> {
    let raw = payload
        .pointer("/input/summary")
        .or_else(|| payload.get("input_summary"))?
        .as_str()?;
    let trimmed = raw.trim_start();
    let without_fence = if let Some(rest) = trimmed.strip_prefix("```") {
        rest.trim_start_matches(|ch: char| ch.is_alphanumeric() || ch == '-' || ch == '_')
            .trim_start()
    } else {
        trimmed
    };
    let normalized = without_fence
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if normalized.is_empty() {
        return None;
    }
    const MAX_TITLE_CHARS: usize = 60;
    if normalized.chars().count() <= MAX_TITLE_CHARS {
        return Some(normalized);
    }
    let mut title = normalized
        .chars()
        .take(MAX_TITLE_CHARS - 1)
        .collect::<String>();
    title = title.trim_end().to_string();
    title.push('…');
    Some(title)
}
