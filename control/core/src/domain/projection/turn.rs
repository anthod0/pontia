use serde_json::{Value, json};

use super::{
    MAX_TURN_INPUT_SUMMARY_CHARS, MAX_TURN_OUTPUT_SUMMARY_CHARS, ProjectionState,
    SessionProjection, TurnProjection,
};
use crate::{
    domain::{DomainEvent, EventType, SessionState, TimelineBoundary, TurnState, TurnTopology},
    error::{Error, Result},
};

impl ProjectionState {
    pub(super) fn abandon_active_turn_for_session_exit(
        &mut self,
        event: &DomainEvent,
    ) -> Result<()> {
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

    pub(super) fn apply_turn(&mut self, event: &DomainEvent, new_state: TurnState) -> Result<()> {
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

        if new_state.is_terminal() {
            self.clear_approval_interaction(&event.session_id, None)?;
        }
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
            TurnState::Completed | TurnState::Failed | TurnState::Abandoned => {
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

    pub(super) fn apply_topology_to_existing_turn(&mut self, event: &DomainEvent) -> Result<()> {
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

    fn active_turn_id(&self, session_id: &str) -> Result<Option<&str>> {
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

    fn validate_topology(&self, event: &DomainEvent, turn_id: &str) -> Result<()> {
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
