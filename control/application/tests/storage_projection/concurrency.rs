use pontia_core::domain::EventType;

use crate::fixture::{event, service};

#[tokio::test]
async fn concurrent_first_events_preserve_distinct_turn_ids() {
    let service = service().await;
    service
        .ingest_reported_event(event(
            "evt_session",
            EventType::SessionCreated,
            "sess_1",
            None,
        ))
        .await
        .unwrap();

    let left = service.clone();
    let right = service.clone();
    let (left_result, right_result) = tokio::join!(
        left.ingest_reported_event(event(
            "evt_left",
            EventType::TurnCompleted,
            "sess_1",
            Some("turn_left"),
        )),
        right.ingest_reported_event(event(
            "evt_right",
            EventType::TurnCompleted,
            "sess_1",
            Some("turn_right"),
        )),
    );
    left_result.unwrap();
    right_result.unwrap();

    assert!(service.get_turn("turn_left").await.unwrap().is_some());
    assert!(service.get_turn("turn_right").await.unwrap().is_some());
}
