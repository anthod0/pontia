use super::fixture::*;
use pontia_application::{EventReportError, ReportedFact};
use pontia_core::domain::{EventSource, EventType, ReportedEvent};
use serde_json::{Value, json};

fn fact(kind: EventType, data: Value) -> ReportedFact {
    ReportedFact {
        session_id: "session".into(),
        turn_id: None,
        fact_type: kind,
        data,
    }
}

#[tokio::test]
async fn compaction_facts_are_persisted_broadcast_and_queried_without_advancing_lifecycle() {
    let fixture = Fixture::new(&SPEC, true).await;
    fixture.seed().await;
    fixture
        .state
        .event_ingest_service()
        .ingest_reported_event(ReportedEvent::new(
            "started-turn".into(),
            "session".into(),
            Some("turn_a".into()),
            EventSource::AgentAdapter,
            SPEC.client_type.into(),
            EventType::TurnStarted,
            json!({}),
        ))
        .await
        .unwrap();
    let before = fixture
        .state
        .event_ingest_service()
        .get_session("session")
        .await
        .unwrap()
        .unwrap();
    let turn_before = fixture
        .state
        .event_ingest_service()
        .get_turn("turn_a")
        .await
        .unwrap()
        .unwrap();
    let mut subscriber = fixture.state.agent_events().subscribe();
    for (kind, data) in [
        (
            EventType::SessionCompactionStarted,
            json!({"operation_id":"auto-1","trigger":"threshold"}),
        ),
        (
            EventType::SessionCompactionCompleted,
            json!({"operation_id":"auto-1","trigger":"threshold","native_compaction_id":"native-1","tokens_before":1234,"tokens_after":200}),
        ),
        (
            EventType::SessionCompactionFailed,
            json!({"operation_id":"auto-2","trigger":"overflow","aborted":true}),
        ),
    ] {
        let result = fixture
            .state
            .event_ingest_service()
            .report_fact(fact(kind, data))
            .await
            .unwrap();
        assert!(result.accepted);
        assert_eq!(result.turn_id, None);
        let broadcast = subscriber.try_recv().unwrap();
        assert_eq!(broadcast.event_type, kind);
        assert_eq!(broadcast.turn_id, None);
        assert_eq!(broadcast.event_id, result.event_id);
        assert!(broadcast.timeline_boundary.is_none());
        assert!(broadcast.topology.is_none());
    }
    let after = fixture
        .state
        .event_ingest_service()
        .get_session("session")
        .await
        .unwrap()
        .unwrap();
    let turn_after = fixture
        .state
        .event_ingest_service()
        .get_turn("turn_a")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.state, before.state);
    assert_eq!(after.current_turn_id, before.current_turn_id);
    assert_eq!(after.metadata, before.metadata);
    assert_eq!(turn_after, turn_before);
    let (status, body) = fixture
        .request("GET", "/api/v1/sessions/session/events", None, true)
        .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let events = body["data"]["events"].as_array().unwrap();
    let compactions: Vec<_> = events
        .iter()
        .filter(|event| {
            event["type"]
                .as_str()
                .is_some_and(|kind| kind.starts_with("session.compaction_"))
        })
        .collect();
    assert_eq!(compactions.len(), 3);
    assert_eq!(compactions[1]["payload"]["tokens_before"], 1234);
    assert_eq!(compactions[1]["payload"]["tokens_after"], 200);
    assert_eq!(compactions[2]["payload"]["aborted"], true);
    assert!(
        compactions[2]["payload"]
            .as_object()
            .unwrap()
            .contains_key("error_message")
    );
    assert!(compactions[2]["payload"]["error_message"].is_null());
}

#[tokio::test]
async fn completed_metadata_is_null_when_the_client_does_not_provide_it() {
    let fixture = Fixture::new(&SPEC, true).await;
    fixture.seed().await;
    for trigger in ["manual", "threshold", "overflow", "unknown"] {
        fixture
            .state
            .event_ingest_service()
            .report_fact(fact(
                EventType::SessionCompactionCompleted,
                json!({"operation_id":trigger,"trigger":trigger}),
            ))
            .await
            .unwrap();
    }
    let events = fixture
        .state
        .queries()
        .list_session_events("session")
        .await
        .unwrap();
    for event in &events[2..] {
        for field in ["native_compaction_id", "tokens_before", "tokens_after"] {
            assert!(event.payload.as_object().unwrap().contains_key(field));
            assert!(event.payload[field].is_null());
        }
    }
}

#[tokio::test]
async fn malformed_compaction_facts_and_summary_copies_are_rejected_without_side_effects() {
    let fixture = Fixture::new(&SPEC, true).await;
    fixture.seed().await;
    let mut subscriber = fixture.state.agent_events().subscribe();
    for (kind, data) in [
        (
            EventType::SessionCompactionStarted,
            json!({"trigger":"manual"}),
        ),
        (
            EventType::SessionCompactionStarted,
            json!({"operation_id":" ","trigger":"manual"}),
        ),
        (
            EventType::SessionCompactionStarted,
            json!({"operation_id":1,"trigger":"manual"}),
        ),
        (
            EventType::SessionCompactionStarted,
            json!({"operation_id":"op","trigger":"guessed"}),
        ),
        (
            EventType::SessionCompactionStarted,
            json!({"operation_id":"op","trigger":null}),
        ),
        (
            EventType::SessionCompactionStarted,
            json!({"operation_id":"op","trigger":"manual","summary":"full summary"}),
        ),
        (
            EventType::SessionCompactionCompleted,
            json!({"operation_id":"op","trigger":"manual","tokens_before":-1}),
        ),
        (
            EventType::SessionCompactionCompleted,
            json!({"operation_id":"op","trigger":"manual","tokens_after":1.5}),
        ),
        (
            EventType::SessionCompactionCompleted,
            json!({"operation_id":"op","trigger":"manual","native_compaction_id":42}),
        ),
        (
            EventType::SessionCompactionFailed,
            json!({"operation_id":"op","trigger":"manual"}),
        ),
        (
            EventType::SessionCompactionFailed,
            json!({"operation_id":"op","trigger":"manual","aborted":"true"}),
        ),
        (
            EventType::SessionCompactionFailed,
            json!({"operation_id":"op","trigger":"manual","aborted":false,"error_message":123}),
        ),
    ] {
        let error = fixture
            .state
            .event_ingest_service()
            .report_fact(fact(kind, data))
            .await
            .unwrap_err();
        assert!(
            matches!(error, EventReportError::InvalidFact(_)),
            "{error:?}"
        );
    }
    let mut turn_fact = fact(
        EventType::SessionCompactionStarted,
        json!({"operation_id":"op","trigger":"manual"}),
    );
    turn_fact.turn_id = Some("turn_wrong".into());
    assert!(matches!(
        fixture
            .state
            .event_ingest_service()
            .report_fact(turn_fact)
            .await,
        Err(EventReportError::InvalidFact(_))
    ));
    assert_eq!(
        fixture
            .state
            .queries()
            .list_session_events("session")
            .await
            .unwrap()
            .len(),
        2
    );
    assert!(matches!(
        subscriber.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn runtime_bound_compaction_facts_cannot_be_reported_by_a_stale_instance() {
    let fixture = Fixture::new(&RUNTIME_BOUND, true).await;
    fixture.seed().await;
    for runtime in [None, Some("stale")] {
        let mut data = json!({"operation_id":"op","trigger":"unknown"});
        if let Some(runtime) = runtime {
            data["runtime_id"] = json!(runtime);
        }
        assert!(
            fixture
                .state
                .event_ingest_service()
                .report_fact(fact(EventType::SessionCompactionStarted, data))
                .await
                .is_err()
        );
    }
    fixture
        .state
        .event_ingest_service()
        .report_fact(fact(
            EventType::SessionCompactionStarted,
            json!({"operation_id":"op","trigger":"unknown","runtime_id":"runtime"}),
        ))
        .await
        .unwrap();
    assert_eq!(
        fixture
            .state
            .queries()
            .list_session_events("session")
            .await
            .unwrap()
            .len(),
        3
    );
}
