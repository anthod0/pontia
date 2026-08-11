use pontia_core::domain::{
    DomainEvent, EventSource, EventType, MAX_TURN_INPUT_SUMMARY_CHARS,
    MAX_TURN_OUTPUT_SUMMARY_CHARS, ProjectionState,
};
use serde_json::json;

use crate::fixture::event;

#[test]
fn reducer_derives_missing_session_title_from_first_turn_input() {
    let mut projection = ProjectionState::default();

    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();
    projection
        .apply(&DomainEvent::new(
            "evt_turn_started_title".to_string(),
            "sess_1".to_string(),
            Some("turn_1".to_string()),
            EventSource::AgentAdapter,
            "pi".to_string(),
            EventType::TurnStarted,
            json!({ "input": { "summary": "  inspect TUI-created task titles\nwith details" } }),
        ))
        .unwrap();

    assert_eq!(
        projection.session("sess_1").unwrap().title.as_deref(),
        Some("inspect TUI-created task titles")
    );
}

#[test]
fn reducer_projects_first_turn_summaries_and_truncates_by_character_count() {
    let mut projection = ProjectionState::default();
    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();

    let input = "界".repeat(MAX_TURN_INPUT_SUMMARY_CHARS + 1);
    let created = DomainEvent::new(
        "evt_turn_created_summary".to_string(),
        "sess_1".to_string(),
        Some("turn_1".to_string()),
        EventSource::ExternalApi,
        "pi".to_string(),
        EventType::TurnCreated,
        json!({ "input": { "summary": input } }),
    );
    projection.apply(&created).unwrap();

    let started = DomainEvent::new(
        "evt_turn_started_summary".to_string(),
        "sess_1".to_string(),
        Some("turn_1".to_string()),
        EventSource::AgentAdapter,
        "pi".to_string(),
        EventType::TurnStarted,
        json!({ "input_summary": "later input" }),
    );
    projection.apply(&started).unwrap();

    let output = "界".repeat(MAX_TURN_OUTPUT_SUMMARY_CHARS + 1);
    let output_event = DomainEvent::new(
        "evt_turn_output_summary".to_string(),
        "sess_1".to_string(),
        Some("turn_1".to_string()),
        EventSource::AgentClient,
        "pi".to_string(),
        EventType::TurnOutput,
        json!({ "output_summary": output }),
    );
    projection.apply(&output_event).unwrap();

    let later_output = DomainEvent::new(
        "evt_turn_output_summary_later".to_string(),
        "sess_1".to_string(),
        Some("turn_1".to_string()),
        EventSource::AgentClient,
        "pi".to_string(),
        EventType::TurnOutput,
        json!({ "output": { "summary": "later output" } }),
    );
    projection.apply(&later_output).unwrap();

    let turn = projection.turn("turn_1").unwrap();
    assert_eq!(
        turn.input_summary.as_deref().unwrap().chars().count(),
        MAX_TURN_INPUT_SUMMARY_CHARS
    );
    assert_eq!(
        turn.output_summary.as_deref().unwrap().chars().count(),
        MAX_TURN_OUTPUT_SUMMARY_CHARS
    );
    assert_eq!(
        turn.input_summary.as_deref(),
        Some("界".repeat(MAX_TURN_INPUT_SUMMARY_CHARS).as_str())
    );
    assert_eq!(
        turn.output_summary.as_deref(),
        Some("界".repeat(MAX_TURN_OUTPUT_SUMMARY_CHARS).as_str())
    );
}
