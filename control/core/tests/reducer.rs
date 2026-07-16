use pontia_core::domain::{
    DomainEvent, EventSource, EventType, MAX_TURN_INPUT_SUMMARY_CHARS,
    MAX_TURN_OUTPUT_SUMMARY_CHARS, ProjectionState, SessionState, TimelineBoundary, TurnState,
    TurnTopology,
};
use serde_json::json;

fn event(event_type: EventType, session_id: &str, turn_id: Option<&str>) -> DomainEvent {
    let event = DomainEvent::new(
        format!("evt_{:?}_{:?}", event_type, turn_id).replace(['.', '"', ' '], "_"),
        session_id.to_string(),
        turn_id.map(str::to_string),
        EventSource::ExternalApi,
        "generic".to_string(),
        event_type,
        json!({}),
    );
    match turn_id {
        Some("turn_2") => event.with_turn_index(2),
        Some(_) => event.with_turn_index(1),
        None => event,
    }
}

#[test]
fn reducer_projects_session_lifecycle_and_turn_busy_idle() {
    let mut projection = ProjectionState::default();

    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::SessionStarting, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::SessionReady, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::TurnCreated, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::TurnQueued, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::TurnStarted, "sess_1", Some("turn_1")))
        .unwrap();

    assert_eq!(
        projection.session("sess_1").unwrap().state,
        SessionState::Busy
    );
    assert_eq!(
        projection
            .session("sess_1")
            .unwrap()
            .current_turn_id
            .as_deref(),
        Some("turn_1")
    );
    assert_eq!(projection.turn("turn_1").unwrap().state, TurnState::Running);

    projection
        .apply(&event(EventType::TurnCompleted, "sess_1", Some("turn_1")))
        .unwrap();

    assert_eq!(
        projection.session("sess_1").unwrap().state,
        SessionState::Idle
    );
    assert_eq!(projection.session("sess_1").unwrap().current_turn_id, None);
    assert_eq!(
        projection.turn("turn_1").unwrap().state,
        TurnState::Completed
    );
}

#[test]
fn reducer_does_not_let_late_events_change_terminal_session_or_turn() {
    let mut projection = ProjectionState::default();

    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::SessionReady, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::TurnCreated, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::TurnStarted, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::TurnCompleted, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::SessionExited, "sess_1", None))
        .unwrap();

    projection
        .apply(&event(EventType::TurnStarted, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::SessionReady, "sess_1", None))
        .unwrap();

    assert_eq!(
        projection.session("sess_1").unwrap().state,
        SessionState::Exited
    );
    assert_eq!(
        projection.turn("turn_1").unwrap().state,
        TurnState::Completed
    );
}

#[test]
fn reducer_derives_missing_session_title_from_first_turn_input() {
    let mut projection = ProjectionState::default();

    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();
    projection
        .apply(
            &DomainEvent::new(
                "evt_turn_started_title".to_string(),
                "sess_1".to_string(),
                Some("turn_1".to_string()),
                EventSource::AgentAdapter,
                "pi".to_string(),
                EventType::TurnStarted,
                json!({ "input": { "summary": "  inspect TUI-created task titles\nwith details" } }),
            )
            .with_turn_index(1),
        )
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
    )
    .with_turn_index(1);
    projection.apply(&created).unwrap();

    let started = DomainEvent::new(
        "evt_turn_started_summary".to_string(),
        "sess_1".to_string(),
        Some("turn_1".to_string()),
        EventSource::AgentAdapter,
        "pi".to_string(),
        EventType::TurnStarted,
        json!({ "input_summary": "later input" }),
    )
    .with_turn_index(1);
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
    )
    .with_turn_index(1);
    projection.apply(&output_event).unwrap();

    let later_output = DomainEvent::new(
        "evt_turn_output_summary_later".to_string(),
        "sess_1".to_string(),
        Some("turn_1".to_string()),
        EventSource::AgentClient,
        "pi".to_string(),
        EventType::TurnOutput,
        json!({ "output": { "summary": "later output" } }),
    )
    .with_turn_index(1);
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

#[test]
fn reducer_rejects_second_active_turn_in_same_session() {
    let mut projection = ProjectionState::default();

    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::SessionReady, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::TurnCreated, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::TurnStarted, "sess_1", Some("turn_1")))
        .unwrap();

    let result = projection.apply(&event(EventType::TurnStarted, "sess_1", Some("turn_2")));

    assert!(result.is_err());
    assert_eq!(
        projection
            .session("sess_1")
            .unwrap()
            .current_turn_id
            .as_deref(),
        Some("turn_1")
    );
    assert!(projection.turn("turn_2").is_none());
}

#[test]
fn reducer_rejects_a_changed_turn_index_during_replay() {
    let mut projection = ProjectionState::default();
    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::TurnCompleted, "sess_1", Some("turn_1")))
        .unwrap();

    let changed = DomainEvent::new(
        "evt_changed_index".to_string(),
        "sess_1".to_string(),
        Some("turn_1".to_string()),
        EventSource::ExternalApi,
        "generic".to_string(),
        EventType::TurnOutput,
        json!({}),
    )
    .with_turn_index(2);
    let error = projection
        .apply(&changed)
        .expect_err("turn index must be immutable during replay");
    assert!(
        error
            .to_string()
            .contains("immutable session_id and turn_index")
    );
}

#[test]
fn reducer_projects_timeline_boundaries_without_losing_the_head() {
    let mut projection = ProjectionState::default();
    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();

    let started = event(EventType::TurnStarted, "sess_1", Some("turn_1"))
        .with_timeline_boundary(TimelineBoundary::head("head-cursor"));
    projection.apply(&started).unwrap();
    assert_eq!(
        projection.turn("turn_1").unwrap().head_cursor.as_deref(),
        Some("head-cursor")
    );
    assert_eq!(projection.turn("turn_1").unwrap().tail_cursor, None);

    let completed = event(EventType::TurnCompleted, "sess_1", Some("turn_1"))
        .with_timeline_boundary(TimelineBoundary::tail("tail-cursor"));
    projection.apply(&completed).unwrap();
    let turn = projection.turn("turn_1").unwrap();
    assert_eq!(turn.head_cursor.as_deref(), Some("head-cursor"));
    assert_eq!(turn.tail_cursor.as_deref(), Some("tail-cursor"));
}

#[test]
fn runtime_binding_is_auxiliary_not_domain_transition() {
    let mut projection = ProjectionState::default();

    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::SessionReady, "sess_1", None))
        .unwrap();
    let before = projection.session("sess_1").unwrap().clone();

    projection.record_runtime_binding("sess_1", "tmux:abc");

    assert_eq!(projection.session("sess_1").unwrap(), &before);
}

#[test]
fn reducer_projects_unknown_root_and_linked_turn_topology() {
    let mut projection = ProjectionState::default();
    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();

    projection
        .apply(&event(EventType::TurnStarted, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::TurnCompleted, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(
            &event(EventType::TurnStarted, "sess_1", Some("turn_2"))
                .with_topology(TurnTopology::Root),
        )
        .unwrap();
    projection
        .apply(&event(EventType::TurnCompleted, "sess_1", Some("turn_2")))
        .unwrap();

    let linked = DomainEvent::new(
        "evt_linked_turn_3".to_string(),
        "sess_1".to_string(),
        Some("turn_3".to_string()),
        EventSource::AgentAdapter,
        "pi".to_string(),
        EventType::TurnStarted,
        json!({}),
    )
    .with_turn_index(3)
    .with_topology(TurnTopology::linked("turn_1"));
    projection.apply(&linked).unwrap();

    assert_eq!(
        projection.turn("turn_1").unwrap().topology,
        TurnTopology::Unknown
    );
    assert_eq!(
        projection.turn("turn_2").unwrap().topology,
        TurnTopology::Root
    );
    assert_eq!(
        projection.turn("turn_3").unwrap().topology,
        TurnTopology::linked("turn_1")
    );
}

#[test]
fn reducer_rejects_invalid_and_conflicting_resolved_topology() {
    fn started(turn_id: &str, session_id: &str, turn_index: i64) -> DomainEvent {
        DomainEvent::new(
            format!("evt_{turn_id}_{session_id}_{turn_index}"),
            session_id.to_string(),
            Some(turn_id.to_string()),
            EventSource::AgentAdapter,
            "generic".to_string(),
            EventType::TurnStarted,
            json!({}),
        )
        .with_turn_index(turn_index)
    }

    let mut projection = ProjectionState::default();
    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::SessionCreated, "sess_2", None))
        .unwrap();
    projection.apply(&started("other", "sess_2", 1)).unwrap();
    projection
        .apply(&event(EventType::TurnCompleted, "sess_2", Some("other")))
        .unwrap();
    projection
        .apply(&started("turn_1", "sess_1", 1).with_topology(TurnTopology::Root))
        .unwrap();
    projection
        .apply(&event(EventType::TurnCompleted, "sess_1", Some("turn_1")))
        .unwrap();

    for (label, invalid) in [
        (
            "self parent",
            started("turn_2", "sess_1", 2).with_topology(TurnTopology::linked("turn_2")),
        ),
        (
            "forward parent",
            started("turn_0", "sess_1", 0).with_topology(TurnTopology::linked("turn_1")),
        ),
        (
            "cross-session parent",
            started("turn_2", "sess_1", 2).with_topology(TurnTopology::linked("other")),
        ),
        (
            "missing parent",
            started("turn_2", "sess_1", 2).with_topology(TurnTopology::linked("missing")),
        ),
    ] {
        assert!(projection.apply(&invalid).is_err(), "{label}");
    }

    let identical = started("turn_1", "sess_1", 1).with_topology(TurnTopology::Root);
    projection.apply(&identical).unwrap();
    let conflict = started("turn_1", "sess_1", 1).with_topology(TurnTopology::linked("turn_1"));
    assert!(projection.apply(&conflict).is_err());
    assert_eq!(
        projection.turn("turn_1").unwrap().topology,
        TurnTopology::Root
    );
}

#[test]
fn topology_enrichment_only_applies_to_started_events_and_never_changes_lifecycle() {
    let mut projection = ProjectionState::default();
    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::TurnStarted, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::TurnCompleted, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::SessionExited, "sess_1", None))
        .unwrap();

    let enrichment =
        event(EventType::TurnStarted, "sess_1", Some("turn_1")).with_topology(TurnTopology::Root);
    projection.apply(&enrichment).unwrap();
    assert_eq!(
        projection.turn("turn_1").unwrap().topology,
        TurnTopology::Root
    );
    assert_eq!(
        projection.turn("turn_1").unwrap().state,
        TurnState::Completed
    );
    assert_eq!(
        projection.session("sess_1").unwrap().state,
        SessionState::Exited
    );

    let invalid =
        event(EventType::TurnCompleted, "sess_1", Some("turn_1")).with_topology(TurnTopology::Root);
    assert!(projection.apply(&invalid).is_err());

    let missing = DomainEvent::new(
        "evt_missing_topology_only".to_string(),
        "sess_1".to_string(),
        Some("turn_missing".to_string()),
        EventSource::AgentAdapter,
        "generic".to_string(),
        EventType::TurnStarted,
        json!({}),
    )
    .with_turn_index(2)
    .with_topology(TurnTopology::Root);
    assert!(projection.apply(&missing).is_err());
    assert!(projection.turn("turn_missing").is_none());
}
