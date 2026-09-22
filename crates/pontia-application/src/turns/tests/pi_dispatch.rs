use std::time::Duration;

use pontia_core::domain::{EventSource, EventType, ReportedEvent};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
};

use crate::{EventIngestService, PiControlService, PublishPiControlEndpoint, TurnCommandService};

async fn setup() -> (SqlitePool, tempfile::TempDir, PiControlService) {
    let root = tempfile::Builder::new().prefix("pd-").tempdir().unwrap();
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
    sqlx::query("INSERT INTO runtime_bindings (session_id,runtime_kind,runtime_instance_id,binding_state,tmux_socket_path,tmux_pane_id,capabilities) VALUES ('sess_pi','pi_tui','rtinst_pi','confirmed','/unused/tmux','%1',?)")
        .bind(serde_json::to_string(&pontia_agent_clients::pi::CAPABILITIES).unwrap()).execute(&pool).await.unwrap();
    let control = PiControlService::new(pool.clone(), root.path().into());
    (pool, root, control)
}

async fn ready(pool: &SqlitePool) {
    EventIngestService::new(pool.clone())
        .ingest_reported_event(ReportedEvent::new(
            "evt_ready".into(),
            "sess_pi".into(),
            None,
            EventSource::AgentClient,
            "pi".into(),
            EventType::SessionReady,
            json!({"runtime_instance_id":"rtinst_pi"}),
        ))
        .await
        .unwrap();
}

#[tokio::test]
async fn pi_input_waits_for_ready_uses_socket_and_leaves_lifecycle_to_client() {
    let (pool, root, control) = setup().await;
    let path = root.path().join("s");
    let listener = UnixListener::bind(&path).unwrap();
    control
        .publish_endpoint(PublishPiControlEndpoint {
            session_id: "sess_pi".into(),
            runtime_instance_id: "rtinst_pi".into(),
            socket_path: path.display().to_string(),
            version: 1,
        })
        .await
        .unwrap();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut stream = BufReader::new(socket);
        let mut line = String::new();
        stream.read_line(&mut line).await.unwrap();
        let hello: Value = serde_json::from_str(&line).unwrap();
        let reply = json!({"version":1,"request_id":hello["request_id"],"result":{"session_id":"sess_pi","runtime_instance_id":"rtinst_pi"}});
        stream
            .get_mut()
            .write_all(format!("{reply}\n").as_bytes())
            .await
            .unwrap();
        line.clear();
        stream.read_line(&mut line).await.unwrap();
        let request: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(request["method"], "submit");
        assert_eq!(request["input"], "hello after ready");
        assert_eq!(request["inbox_message_id"], "msg_one");
        let reply =
            json!({"version":1,"request_id":request["request_id"],"result":{"accepted":true}});
        stream
            .get_mut()
            .write_all(format!("{reply}\n").as_bytes())
            .await
            .unwrap();
    });
    let service = TurnCommandService::new(crate::EventIngestService::new(pool.clone()))
        .with_pi_control(control);
    let dispatch = tokio::spawn(async move {
        service
            .create_and_dispatch_turn(
                "sess_pi",
                "hello after ready".into(),
                json!({"inbox_message_id":"msg_one"}),
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!dispatch.is_finished());
    ready(&pool).await;
    assert!(
        tokio::time::timeout(Duration::from_secs(2), dispatch)
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .is_none()
    );
    server.await.unwrap();
    let turns: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns")
        .fetch_one(&pool)
        .await
        .unwrap();
    let contexts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pending_turn_contexts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(turns, 0);
    assert_eq!(
        contexts, 0,
        "socket submissions carry metadata directly, never through a stale claim"
    );
}

#[tokio::test]
async fn missing_pi_endpoint_rejects_delivery_without_creating_a_turn_or_pending_context() {
    let (pool, _root, control) = setup().await;
    ready(&pool).await;
    let error = TurnCommandService::new(crate::EventIngestService::new(pool.clone()))
        .with_pi_control(control)
        .create_and_dispatch_turn("sess_pi", "not delivered".into(), json!({}))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("no current Pi control endpoint"));
    for table in ["turns", "pending_turn_contexts"] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}
