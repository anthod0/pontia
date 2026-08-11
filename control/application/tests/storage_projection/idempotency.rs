use pontia_core::domain::EventType;

use crate::fixture::{event, service, service_with_agent_events};

#[tokio::test]
async fn duplicate_agent_event_is_not_rebroadcast() {
    let (service, broker) = service_with_agent_events().await;
    let mut subscriber = broker.subscribe();
    let reported = event(
        "evt_duplicate",
        EventType::SessionCreated,
        "sess_duplicate",
        None,
    );

    service
        .ingest_reported_event(reported.clone())
        .await
        .unwrap();
    assert_eq!(
        subscriber
            .recv()
            .await
            .expect("first committed event")
            .event_id,
        "evt_duplicate"
    );

    let duplicate = service.ingest_reported_event(reported).await.unwrap();
    assert!(duplicate.duplicate);
    assert!(matches!(
        subscriber.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn duplicate_event_id_is_idempotent() {
    let service = service().await;
    let first = event("evt_same", EventType::SessionCreated, "sess_1", None);

    let first_result = service.ingest_reported_event(first.clone()).await.unwrap();
    let second_result = service.ingest_reported_event(first).await.unwrap();

    assert!(!first_result.duplicate);
    assert!(second_result.duplicate);
    assert_eq!(first_result.state_version, second_result.state_version);
    assert_eq!(service.list_events("sess_1").await.unwrap().len(), 1);
}
