use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;

struct ReadyRuntimeEvent;
impl crate::client_contract::ClientEventInterpreter for ReadyRuntimeEvent {
    fn accompanying_runtime_event(
        &self,
        event: &pontia_core::domain::DomainEvent,
    ) -> pontia_core::Result<Option<pontia_core::domain::DomainEvent>> {
        if event.event_type != pontia_core::domain::EventType::SessionReady {
            return Ok(None);
        }
        let mut runtime = event.clone();
        runtime.event_id = format!("{}:runtime", event.event_id);
        runtime.event_type = pontia_core::domain::EventType::RuntimeReady;
        runtime.payload["role"] = serde_json::json!("generic_control");
        Ok(Some(runtime))
    }
}

#[tokio::test]
async fn initial_input_uses_the_injected_channel_after_ready() {
    let root = tempfile::Builder::new().prefix("pi-").tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("test.db").display()
    ))
    .await
    .unwrap();
    run_migrations(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO sessions (session_id,client_type,state) VALUES ('sess_pi','test-channel','starting')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO session_runtimes(session_id, runtime_id, tmux_socket_path, tmux_pane_id, role, state, created_at) VALUES ('sess_pi', 'rtinst_pi', '/unused/tmux', '%1', 'generic_control', 'running', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))"#).execute(&pool).await.unwrap();
    sqlx::query("UPDATE session_runtimes SET state='starting' WHERE session_id='sess_pi'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO agent_bindings (id,session_id,client_type,launch_cwd,client_session_key,metadata) VALUES ('binding_pi','sess_pi','test-channel','/unused','native_pi','{}')").execute(&pool).await.unwrap();
    let state = crate::AgentControl::builder(pool.clone(), root.path().into())
        .clients({
            let mut clients = crate::clients::testing::clients();
            let mut registration = clients.get("test-channel").unwrap().clone();
            registration.events = Some(std::sync::Arc::new(ReadyRuntimeEvent));
            clients.register(registration);
            clients
        })
        .build();
    let control = state.client_control();
    let channel = crate::clients::testing::channel();
    control
        .attach(
            "test-channel",
            "sess_pi",
            "rtinst_pi",
            "native_pi",
            channel.clone(),
        )
        .await
        .unwrap();
    let events = state.event_ingest_service();
    let delivery = state
        .inbox_commands()
        .send(
            "sess_pi",
            crate::InputPayload::from("initial input"),
            crate::SendMode::Queue,
        )
        .await
        .unwrap();
    let crate::SendOutcome::Queued { message_id } = delivery else {
        panic!("input must remain queued until the Session is ready");
    };
    // The client's ready fact commits Session and runtime readiness together.
    events
        .ingest_reported_event(pontia_core::domain::ReportedEvent::new(
            "evt_ready".into(),
            "sess_pi".into(),
            None,
            pontia_core::domain::EventSource::AgentClient,
            "test-channel".into(),
            pontia_core::domain::EventType::SessionReady,
            json!({"runtime_id":"rtinst_pi"}),
        ))
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let delivered = state
                .inbox_commands()
                .get_message("sess_pi", &message_id)
                .await
                .unwrap()
                .is_none();
            if delivered {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(*channel.input.lock().unwrap(), vec!["initial input"]);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert!(
        state
            .inbox_commands()
            .get_message("sess_pi", &message_id)
            .await
            .unwrap()
            .is_none()
    );
    state.inbox_commands().stop_scheduling().await;
}
