use crate::TurnCommandService;
use crate::{EventIngestService, PiControlService};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

#[tokio::test]
async fn initial_pi_input_uses_the_shared_socket_after_ready() {
    let root = tempfile::Builder::new().prefix("pi-").tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("test.db").display()
    ))
    .await
    .unwrap();
    run_migrations(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO sessions (session_id,client_type,state) VALUES ('sess_pi','pi','starting')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO runtime_bindings (session_id,runtime_kind,runtime_instance_id,binding_state,tmux_socket_path,tmux_pane_id,capabilities) VALUES ('sess_pi','pi_tui','rtinst_pi','confirmed','/unused/tmux','%1','{\"accept_task\":true}')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO agent_bindings (id,session_id,client_type,launch_cwd,client_session_key,metadata) VALUES ('binding_pi','sess_pi','pi','/unused','native_pi','{}')").execute(&pool).await.unwrap();
    let control = PiControlService::new(pool.clone(), root.path().into());
    let (daemon_socket, socket) = UnixStream::pair().unwrap();
    let (peer, _requests) = pontia_runtime::pi_control::PiRpcPeer::new(daemon_socket);
    control
        .attach("sess_pi", "rtinst_pi", "native_pi", peer)
        .await
        .unwrap();
    let server = tokio::spawn(async move {
        let mut stream = BufReader::new(socket);
        let mut line = String::new();
        stream.read_line(&mut line).await.unwrap();
        let request: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(request["method"], "submit");
        assert_eq!(request["params"]["input"], "initial input");
        let reply = json!({"jsonrpc":"2.0","id":request["id"],"result":{"accepted":true}});
        stream
            .get_mut()
            .write_all(format!("{reply}\n").as_bytes())
            .await
            .unwrap();
    });
    let events = EventIngestService::new(pool.clone()).with_pi_control(control);
    let service = TurnCommandService::new(events.clone());
    let dispatch = tokio::spawn(async move {
        service
            .dispatch_initial(
                &crate::runtime::control_target::ControlTarget {
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
            "pi".into(),
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
    server.await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert!(events.inbox_scheduler().awaiting_initial("sess_pi"));
    crate::RuntimeObservationService::new(events.clone())
        .observe_session("sess_pi")
        .await
        .unwrap();
    assert!(
        !events.inbox_scheduler().awaiting_initial("sess_pi"),
        "a confirmed process exit must release the initial delivery gate"
    );
}
