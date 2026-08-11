use pontia_core::domain::{EventType, ProjectionState, TimelineBoundary};

use crate::fixture::event;

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
