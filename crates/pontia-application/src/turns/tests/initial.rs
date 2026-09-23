use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;

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
    sqlx::query("INSERT INTO runtime_bindings (session_id,runtime_kind,runtime_instance_id,binding_state,tmux_socket_path,tmux_pane_id,capabilities) VALUES ('sess_pi','pi_tui','rtinst_pi','confirmed','/unused/tmux','%1','{\"accept_task\":true}')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO agent_bindings (id,session_id,client_type,launch_cwd,client_session_key,metadata) VALUES ('binding_pi','sess_pi','test-channel','/unused','native_pi','{}')").execute(&pool).await.unwrap();
    let state = crate::AppState::builder(pool.clone(), root.path().into())
        .clients(crate::clients::testing::clients())
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
    let service = state.turn_commands();
    let scheduler = service.scheduler.clone();
    let dispatch = tokio::spawn(async move {
        service
            .dispatch_initial(
                &crate::runtime::ControlTarget {
                    session_id: "sess_pi".into(),
                    runtime_instance_id: Some("rtinst_pi".into()),
                },
                "initial input",
                &json!({}),
                None,
            )
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(!dispatch.is_finished());
    events
        .ingest_reported_event(pontia_core::domain::ReportedEvent::new(
            "evt_ready".into(),
            "sess_pi".into(),
            None,
            pontia_core::domain::EventSource::AgentClient,
            "test-channel".into(),
            pontia_core::domain::EventType::SessionReady,
            json!({"runtime_instance_id":"rtinst_pi"}),
        ))
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), dispatch)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(*channel.input.lock().unwrap(), vec!["initial input"]);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert!(scheduler.awaiting_initial("sess_pi"));
    state
        .runtime_observer()
        .observe_session("sess_pi")
        .await
        .unwrap();
    assert!(
        !scheduler.awaiting_initial("sess_pi"),
        "a confirmed process exit must release the initial delivery gate"
    );
}
