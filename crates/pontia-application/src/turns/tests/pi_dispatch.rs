use std::time::Duration;

use pontia_core::domain::{EventSource, EventType, ReportedEvent};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

use crate::{EventIngestService, PiControlService, TurnCommandService};

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
    sqlx::query("INSERT INTO agent_bindings (id,session_id,client_type,launch_cwd,client_session_key,metadata) VALUES ('binding_pi','sess_pi','pi','/unused','native_pi','{}')").execute(&pool).await.unwrap();
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
    let (pool, _root, control) = setup().await;
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
        assert_eq!(request["params"]["input"], "hello after ready");
        assert_eq!(request["params"]["inbox_message_id"], "msg_one");
        let reply = json!({"jsonrpc":"2.0","id":request["id"],"result":{"accepted":true}});
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
    assert_eq!(turns, 0);
}

#[tokio::test]
async fn missing_pi_connection_rejects_delivery_without_creating_a_turn() {
    let (pool, _root, control) = setup().await;
    ready(&pool).await;
    let error = TurnCommandService::new(crate::EventIngestService::new(pool.clone()))
        .with_pi_control(control)
        .create_and_dispatch_turn("sess_pi", "not delivered".into(), json!({}))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("no current Pi connection"));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
