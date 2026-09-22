use pontia_application::app::VolatileEventBroker;
use pontia_core::domain::{DomainEvent, EventSource, EventType};
use serde_json::json;
use tokio::time::{Duration, timeout};

fn message_updated_event(event_id: &str, session_id: &str) -> DomainEvent {
    DomainEvent::new(
        event_id.to_string(),
        session_id.to_string(),
        None,
        EventSource::AgentClient,
        "pi".to_string(),
        EventType::SessionMessageUpdated,
        json!({ "binding_id": "bind-1" }),
    )
}

#[tokio::test]
async fn debounced_publish_waits_until_100ms_after_latest_session_message_update() {
    let broker = VolatileEventBroker::default();
    let mut subscriber = broker.subscribe();

    broker.publish_debounced_session_message_updated(message_updated_event("evt-1", "sess-1"));
    tokio::time::sleep(Duration::from_millis(90)).await;
    broker.publish_debounced_session_message_updated(message_updated_event("evt-2", "sess-1"));

    assert!(
        timeout(Duration::from_millis(90), subscriber.recv())
            .await
            .is_err()
    );

    let event = timeout(Duration::from_millis(30), subscriber.recv())
        .await
        .expect("debounced event should publish")
        .expect("broker should stay open");

    assert_eq!(event.event_id, "evt-2");
    assert_eq!(event.session_id, "sess-1");
    assert_eq!(event.event_type, EventType::SessionMessageUpdated);
}
