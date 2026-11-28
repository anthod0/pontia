//! Opt into the `conformance` feature in a client's dev-dependencies.
//! Drive the real integration with its native test peer, then call these checks.

use crate::{AgentBindingService, AgentControl, SetSessionReasoningEffortRequest};
use pontia_core::{
    Error,
    domain::{EventType, TimelineBoundary},
};

/// Check a fresh session after its first ready report (including transport retries).
pub async fn assert_first_ready(app: &AgentControl, session: &str) {
    let events = app
        .event_ingest_service()
        .list_events(session)
        .await
        .unwrap();
    let mut ready = events
        .iter()
        .filter(|event| event.event_type == EventType::SessionReady);
    let event = ready.next().expect("missing session.ready");
    assert!(ready.next().is_none(), "duplicate initial session.ready");
    assert!(event.turn_id.is_none());
    let binding = AgentBindingService::new(app.db())
        .binding_for_session(session)
        .await
        .unwrap()
        .expect("ready session must have a native binding");
    assert!(!binding.client_session_key.trim().is_empty());
    assert_eq!(event.client_type, binding.client_type);
    assert_eq!(
        event.payload["client_session_key"].as_str(),
        Some(binding.client_session_key.as_str())
    );
}

/// Check a completed turn whose native start and completion both supplied boundary evidence.
pub async fn assert_completed_turn_boundaries(app: &AgentControl, session: &str, turn: &str) {
    let service = app.event_ingest_service();
    let events = service.list_events(session).await.unwrap();
    let lifecycle: Vec<_> = events
        .iter()
        .filter(|event| {
            event.turn_id.as_deref() == Some(turn)
                && matches!(
                    event.event_type,
                    EventType::TurnStarted | EventType::TurnCompleted
                )
        })
        .collect();
    let [start, end] = lifecycle.as_slice() else {
        panic!("expected one start and one completion for {turn}")
    };
    assert_eq!(start.event_type, EventType::TurnStarted);
    assert_eq!(end.event_type, EventType::TurnCompleted);
    let Some(TimelineBoundary::Head { cursor: head }) = &start.timeline_boundary else {
        panic!("missing head boundary on turn.started for {turn}")
    };
    let Some(TimelineBoundary::Tail { cursor: tail }) = &end.timeline_boundary else {
        panic!("missing tail boundary on turn.completed for {turn}")
    };
    assert!(!head.is_empty() && !tail.is_empty());
    let projection = service.get_turn(turn).await.unwrap().unwrap();
    assert_eq!(projection.head_cursor.as_ref(), Some(head));
    assert_eq!(projection.tail_cursor.as_ref(), Some(tail));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityDenial {
    Unsupported,
    Unavailable,
}

/// Exercise capability_state through the session query and a public control command.
/// Use a peer without effort support, or disconnect a peer that supports it.
pub async fn assert_reasoning_effort_denied(
    app: &AgentControl,
    session: &str,
    expected: CapabilityDenial,
) {
    let view = app.queries().get_session(session).await.unwrap().unwrap();
    assert!(!view.capabilities.reasoning_effort_control);
    let error = app
        .session_commands()
        .set_reasoning_effort(
            session,
            SetSessionReasoningEffortRequest {
                reasoning_effort: "high".into(),
            },
        )
        .await
        .unwrap_err();
    let actual = match error {
        Error::Unsupported(_) => CapabilityDenial::Unsupported,
        Error::Unavailable(_) => CapabilityDenial::Unavailable,
        other => panic!("unexpected reasoning effort error for {session}: {other}"),
    };
    assert_eq!(actual, expected);
}
