use pontia_core::domain::{EventSource, EventType, ReportedEvent, SessionState};
use serde_json::json;

use crate::fixture::{event, service, service_with_agent_events};

struct CoupledEvents;
impl pontia_application::client_contract::ClientEventInterpreter for CoupledEvents {
    fn accompanying_runtime_event(
        &self,
        event: &pontia_core::domain::DomainEvent,
    ) -> pontia_core::Result<Option<pontia_core::domain::DomainEvent>> {
        let kind = match event.event_type {
            EventType::SessionStarting => EventType::RuntimeStarting,
            EventType::SessionReady => EventType::RuntimeReady,
            _ => return Ok(None),
        };
        let mut runtime = event.clone();
        runtime.event_id = format!("{}:runtime", event.event_id);
        runtime.event_type = kind;
        runtime.payload["session_event_id"] = json!(event.event_id);
        Ok(Some(runtime))
    }
}

fn coupled_clients() -> pontia_application::clients::ClientRegistry {
    static SPEC: pontia_application::client_contract::AgentClientSpec =
        pontia_application::client_contract::AgentClientSpec {
            client_type: "pi",
            adapter: pontia_application::client_contract::AgentClientAdapter {
                lifecycle: pontia_application::client_contract::SessionLifecycleBehavior {
                    coupled_runtime: true,
                    ..pontia_application::client_contract::SessionLifecycleBehavior::DEFAULT
                },
                ..pontia_application::client_contract::TEST_SPEC.adapter
            },
            ..pontia_application::client_contract::TEST_SPEC
        };
    let mut client = pontia_application::client_contract::test_registration();
    client.spec = &SPEC;
    client.events = Some(std::sync::Arc::new(CoupledEvents));
    let mut clients = pontia_application::clients::ClientRegistry::default();
    clients.register(client);
    clients
}

fn runtime_event(
    event_id: &str,
    client_type: &str,
    event_type: EventType,
    session_id: &str,
    runtime_id: &str,
) -> ReportedEvent {
    ReportedEvent::new(
        event_id.to_string(),
        session_id.to_string(),
        None,
        EventSource::RuntimeManager,
        client_type.to_string(),
        event_type,
        json!({"runtime_id": runtime_id}),
    )
}

#[tokio::test]
async fn runtime_events_project_only_the_named_runtime() {
    let service = service().await;
    service
        .ingest_reported_event(event(
            "evt_runtime_session",
            EventType::SessionCreated,
            "sess_runtime",
            None,
        ))
        .await
        .unwrap();

    service
        .ingest_reported_event(runtime_event(
            "evt_runtime_a_ready",
            "generic",
            EventType::RuntimeReady,
            "sess_runtime",
            "runtime_a",
        ))
        .await
        .unwrap();
    let created_at: String = sqlx::query_scalar(
        "SELECT created_at FROM session_runtimes WHERE runtime_id = 'runtime_a'",
    )
    .fetch_one(&service.db())
    .await
    .unwrap();
    service
        .ingest_reported_event(runtime_event(
            "evt_runtime_b_starting",
            "generic",
            EventType::RuntimeStarting,
            "sess_runtime",
            "runtime_b",
        ))
        .await
        .unwrap();
    service
        .ingest_reported_event(runtime_event(
            "evt_runtime_a_exited",
            "generic",
            EventType::RuntimeExited,
            "sess_runtime",
            "runtime_a",
        ))
        .await
        .unwrap();

    let invalid_revival = service
        .ingest_reported_event(runtime_event(
            "evt_runtime_a_ready_after_exit",
            "generic",
            EventType::RuntimeReady,
            "sess_runtime",
            "runtime_a",
        ))
        .await;
    assert!(invalid_revival.is_err());

    let states: Vec<(String, String)> = sqlx::query_as(
        "SELECT runtime_id, state FROM session_runtimes WHERE session_id = ? ORDER BY runtime_id",
    )
    .bind("sess_runtime")
    .fetch_all(&service.db())
    .await
    .unwrap();
    assert_eq!(
        states,
        vec![
            ("runtime_a".to_string(), "exited".to_string()),
            ("runtime_b".to_string(), "starting".to_string()),
        ]
    );
    assert_eq!(
        service
            .get_session("sess_runtime")
            .await
            .unwrap()
            .unwrap()
            .state,
        SessionState::Created
    );

    service
        .ingest_reported_event(runtime_event(
            "evt_runtime_a_restarting",
            "generic",
            EventType::RuntimeStarting,
            "sess_runtime",
            "runtime_a",
        ))
        .await
        .unwrap();
    let restarted: (String, String) = sqlx::query_as(
        "SELECT state, created_at FROM session_runtimes WHERE runtime_id = 'runtime_a'",
    )
    .fetch_one(&service.db())
    .await
    .unwrap();
    assert_eq!(restarted, ("starting".to_string(), created_at));
}

#[tokio::test]
async fn runtime_exit_rejects_an_unknown_runtime_without_changing_the_session() {
    let service = service().await;
    service
        .ingest_reported_event(event(
            "evt_unknown_runtime_session",
            EventType::SessionCreated,
            "sess_unknown_runtime",
            None,
        ))
        .await
        .unwrap();

    let result = service
        .ingest_reported_event(runtime_event(
            "evt_unknown_runtime_exit",
            "generic",
            EventType::RuntimeExited,
            "sess_unknown_runtime",
            "runtime_missing",
        ))
        .await;

    assert!(result.is_err());
    assert_eq!(
        service
            .list_events("sess_unknown_runtime")
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        service
            .get_session("sess_unknown_runtime")
            .await
            .unwrap()
            .unwrap()
            .state,
        SessionState::Created
    );
}

#[tokio::test]
async fn pi_session_fact_persists_and_broadcasts_its_runtime_event_in_order_once() {
    let (fixture, broker) = service_with_agent_events().await;
    let service = (*fixture).clone().with_clients(coupled_clients());
    service
        .ingest_reported_event(ReportedEvent::new(
            "evt_pi_created".to_string(),
            "sess_pi".to_string(),
            None,
            EventSource::ExternalApi,
            "pi".to_string(),
            EventType::SessionCreated,
            json!({}),
        ))
        .await
        .unwrap();
    let mut subscriber = broker.subscribe();
    let starting = runtime_event(
        "evt_pi_starting",
        "pi",
        EventType::SessionStarting,
        "sess_pi",
        "runtime_pi",
    );

    let first = service
        .ingest_reported_event(starting.clone())
        .await
        .unwrap();
    assert_eq!(first.state_version, 3);
    let session_notice = subscriber.recv().await.unwrap();
    let runtime_notice = subscriber.recv().await.unwrap();
    assert_eq!(session_notice.event_type, EventType::SessionStarting);
    assert_eq!(runtime_notice.event_type, EventType::RuntimeStarting);
    assert_eq!(runtime_notice.payload["runtime_id"], "runtime_pi");
    assert_eq!(
        runtime_notice.payload["session_event_id"],
        "evt_pi_starting"
    );

    let duplicate = service.ingest_reported_event(starting).await.unwrap();
    assert!(duplicate.duplicate);
    assert_eq!(duplicate.state_version, first.state_version);
    assert!(matches!(
        subscriber.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));

    let events = service.list_events("sess_pi").await.unwrap();
    assert_eq!(
        events
            .iter()
            .map(|event| event.event_type)
            .collect::<Vec<_>>(),
        vec![
            EventType::SessionCreated,
            EventType::SessionStarting,
            EventType::RuntimeStarting,
        ]
    );
}

#[tokio::test]
async fn pi_session_and_runtime_events_roll_back_together() {
    let fixture = service().await;
    let service = (*fixture).clone().with_clients(coupled_clients());
    service
        .ingest_reported_event(ReportedEvent::new(
            "evt_pi_rollback_created".to_string(),
            "sess_pi_rollback".to_string(),
            None,
            EventSource::ExternalApi,
            "pi".to_string(),
            EventType::SessionCreated,
            json!({}),
        ))
        .await
        .unwrap();
    service
        .ingest_reported_event(runtime_event(
            "evt_pi_rollback_starting",
            "pi",
            EventType::SessionStarting,
            "sess_pi_rollback",
            "runtime_pi_rollback",
        ))
        .await
        .unwrap();
    sqlx::raw_sql(
        "CREATE TRIGGER reject_runtime_ready BEFORE INSERT ON events WHEN NEW.event_type = 'runtime.ready' BEGIN SELECT RAISE(ABORT, 'injected runtime event failure'); END",
    )
    .execute(&service.db())
    .await
    .unwrap();

    let result = service
        .ingest_reported_event(runtime_event(
            "evt_pi_rollback_ready",
            "pi",
            EventType::SessionReady,
            "sess_pi_rollback",
            "runtime_pi_rollback",
        ))
        .await;

    assert!(result.is_err());
    let states: (String, String) = sqlx::query_as(
        "SELECT s.state, r.state FROM sessions s JOIN session_runtimes r USING(session_id) WHERE s.session_id = ?",
    )
    .bind("sess_pi_rollback")
    .fetch_one(&service.db())
    .await
    .unwrap();
    assert_eq!(states, ("starting".to_string(), "starting".to_string()));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM events WHERE session_id = ? AND event_type IN ('session.ready', 'runtime.ready')",
        )
        .bind("sess_pi_rollback")
        .fetch_one(&service.db())
        .await
        .unwrap(),
        0
    );
}
