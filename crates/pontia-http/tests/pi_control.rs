use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use pontia_application::{AppState, EventIngestService, pi_ipc::PiIpcListener};
use pontia_runtime::pi_control::PiRpcPeer;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::UnixStream,
    process::Command,
};
use tower::ServiceExt;

async fn state() -> (AppState, tempfile::TempDir) {
    let root = tempfile::Builder::new().prefix("pc-").tempdir().unwrap();
    let pool = connect_sqlite(&format!("sqlite://{}", root.path().join("db").display()))
        .await
        .unwrap();
    run_migrations(&pool).await.unwrap();
    let state = AppState::builder(pool, root.path().into())
        .external_api_token(Some("token".into()))
        .build();
    (state, root)
}

async fn bind(state: &AppState, session: &str, runtime: &str) {
    sqlx::query("INSERT INTO sessions (session_id,client_type,state) VALUES (?,'pi','idle') ON CONFLICT DO NOTHING").bind(session).execute(&state.db()).await.unwrap();
    sqlx::query("INSERT INTO runtime_bindings (session_id,runtime_kind,runtime_instance_id,binding_state,tmux_socket_path,tmux_pane_id,capabilities) VALUES (?,'tmux',?,'confirmed','/unused/tmux','%1',?) ON CONFLICT(session_id) DO UPDATE SET runtime_instance_id=excluded.runtime_instance_id")
        .bind(session).bind(runtime).bind(serde_json::to_string(&pontia_agent_clients::pi::CAPABILITIES).unwrap()).execute(&state.db()).await.unwrap();
    sqlx::query("INSERT INTO agent_bindings (id,session_id,client_type,launch_cwd,client_session_key,metadata) VALUES (?,?,'pi','/unused',?,'{}') ON CONFLICT DO NOTHING")
        .bind(format!("binding_{session}")).bind(session).bind(format!("native_{session}")).execute(&state.db()).await.unwrap();
}

async fn attach(
    state: &AppState,
    session: &str,
    runtime: &str,
) -> (
    std::sync::Arc<PiRpcPeer>,
    tokio::sync::mpsc::Receiver<pontia_runtime::pi_control::RpcRequest>,
) {
    let (server, client) = UnixStream::pair().unwrap();
    let (peer, requests) = PiRpcPeer::new(server);
    state
        .pi_control()
        .attach(session, runtime, &format!("native_{session}"), peer.clone())
        .await
        .unwrap();
    // Keep the peer's inbound receiver alive while the test handles daemon requests.
    drop(requests);
    let (client, requests) = PiRpcPeer::new(client);
    (client, requests)
}

#[tokio::test]
async fn replacement_and_exit_fence_connections_without_synthesizing_facts() {
    let (state, _root) = state().await;
    bind(&state, "sess_pi", "rt_old").await;
    let (old, mut requests) = attach(&state, "sess_pi", "rt_old").await;
    let ping = {
        let control = state.pi_control();
        tokio::spawn(async move { control.ping("sess_pi", "rt_old").await })
    };
    let request = requests.recv().await.unwrap();
    old.reply(request.id, json!({"pong":true})).await.unwrap();
    ping.await.unwrap().unwrap();
    let blocked = {
        let control = state.pi_control();
        tokio::spawn(async move { control.submit("sess_pi", "rt_old", "uncertain", None).await })
    };
    requests.recv().await.unwrap();
    bind(&state, "sess_pi", "rt_new").await;
    let (new, _requests) = attach(&state, "sess_pi", "rt_new").await;
    assert!(matches!(
        blocked.await.unwrap(),
        Err(pontia_core::Error::ControlUnknown(_))
    ));
    assert!(state.pi_control().ping("sess_pi", "rt_old").await.is_err());
    EventIngestService::new(state.db())
        .with_pi_control(state.pi_control())
        .ingest_reported_event(pontia_core::domain::ReportedEvent::new(
            "evt_exit".into(),
            "sess_pi".into(),
            None,
            pontia_core::domain::EventSource::AgentClient,
            "pi".into(),
            pontia_core::domain::EventType::SessionExited,
            json!({"runtime_instance_id":"rt_new"}),
        ))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), new.closed())
        .await
        .unwrap();
    assert!(!state.pi_control().available("sess_pi").await.unwrap());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn real_pi_client_reconnects_after_daemon_restart_and_delivers_external_input() {
    let (state, root) = state().await;
    bind(&state, "sess_input", "rt_input").await;
    let listener = PiIpcListener::bind(root.path()).await.unwrap();
    let task = tokio::spawn(listener.run(state.clone(), state.shutdown().subscribe()));
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../clients/pi/test/control-server.mjs");
    let mut child = Command::new("node")
        .arg(fixture)
        .arg(root.path())
        .args(["sess_input", "rt_input", "native_sess_input"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut line = String::new();
    tokio::time::timeout(
        Duration::from_secs(5),
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut line),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(line.trim(), "connected");
    state
        .pi_control()
        .ping("sess_input", "rt_input")
        .await
        .unwrap();
    state.shutdown().notify();
    task.await.unwrap().unwrap();
    state.pi_control().close().await;
    let restarted = AppState::builder(state.db(), root.path().into())
        .external_api_token(Some("token".into()))
        .build();
    EventIngestService::new(restarted.db())
        .ingest_reported_event(pontia_core::domain::ReportedEvent::new(
            "evt_ready".into(),
            "sess_input".into(),
            None,
            pontia_core::domain::EventSource::AgentClient,
            "pi".into(),
            pontia_core::domain::EventType::SessionReady,
            json!({"runtime_instance_id":"rt_input"}),
        ))
        .await
        .unwrap();
    let response = pontia_http::router(restarted.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/external/v1/sessions/sess_input/inbox/messages")
                .header("content-type", "application/json")
                .header("authorization", "Bearer token")
                .body(Body::from(json!({"input":"first\n你好"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["data"]["inbox_message"]["state"], "pending");
    let listener = PiIpcListener::bind(root.path()).await.unwrap();
    let task = tokio::spawn(listener.run(restarted.clone(), restarted.shutdown().subscribe()));
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let status: String =
                sqlx::query_scalar("SELECT state FROM inbox_messages WHERE message_id=?")
                    .bind(
                        body["data"]["inbox_message"]["message_id"]
                            .as_str()
                            .unwrap(),
                    )
                    .fetch_one(&state.db())
                    .await
                    .unwrap();
            if status == "dispatched" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let delivered: Value =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("messages.jsonl")).unwrap())
            .unwrap();
    assert_eq!(
        delivered,
        json!({"input":"first\n你好","inboxMessageId":body["data"]["inbox_message"]["message_id"]})
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns")
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert_eq!(count, 0);
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE event_type IN ('session.exited','turn.started')",
    )
    .fetch_one(&state.db())
    .await
    .unwrap();
    assert_eq!(count, 0);
    child.stdin.take();
    assert!(child.wait().await.unwrap().success());
    restarted.shutdown().notify();
    task.await.unwrap().unwrap();
    restarted.pi_control().close().await;
}
