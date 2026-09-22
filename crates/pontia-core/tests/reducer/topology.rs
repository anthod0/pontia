use pontia_core::domain::{
    DomainEvent, EventSource, EventType, ProjectionState, SessionState, TurnState, TurnTopology,
};
use serde_json::json;

use crate::fixture::event;

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
    fn started(turn_id: &str, session_id: &str) -> DomainEvent {
        DomainEvent::new(
            format!("evt_{turn_id}_{session_id}"),
            session_id.to_string(),
            Some(turn_id.to_string()),
            EventSource::AgentAdapter,
            "generic".to_string(),
            EventType::TurnStarted,
            json!({}),
        )
    }

    let mut projection = ProjectionState::default();
    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::SessionCreated, "sess_2", None))
        .unwrap();
    projection.apply(&started("other", "sess_2")).unwrap();
    projection
        .apply(&event(EventType::TurnCompleted, "sess_2", Some("other")))
        .unwrap();
    projection
        .apply(&started("turn_1", "sess_1").with_topology(TurnTopology::Root))
        .unwrap();
    projection
        .apply(&event(EventType::TurnCompleted, "sess_1", Some("turn_1")))
        .unwrap();

    for (label, invalid) in [
        (
            "self parent",
            started("turn_2", "sess_1").with_topology(TurnTopology::linked("turn_2")),
        ),
        (
            "forward parent",
            started("turn_0", "sess_1").with_topology(TurnTopology::linked("turn_1")),
        ),
        (
            "cross-session parent",
            started("turn_2", "sess_1").with_topology(TurnTopology::linked("other")),
        ),
        (
            "missing parent",
            started("turn_2", "sess_1").with_topology(TurnTopology::linked("missing")),
        ),
    ] {
        assert!(projection.apply(&invalid).is_err(), "{label}");
    }

    let identical = started("turn_1", "sess_1").with_topology(TurnTopology::Root);
    projection.apply(&identical).unwrap();
    let conflict = started("turn_1", "sess_1").with_topology(TurnTopology::linked("turn_1"));
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
    .with_topology(TurnTopology::Root);
    assert!(projection.apply(&missing).is_err());
    assert!(projection.turn("turn_missing").is_none());
}
