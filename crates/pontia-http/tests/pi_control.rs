use std::{os::unix::fs::PermissionsExt, path::Path, process::Stdio, time::Duration};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use pontia_application::AppState;
use pontia_runtime::pi_control::{PiControlConnection, PiControlEndpoint};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
};
use tower::ServiceExt;

struct PiProcess {
    child: Child,
    endpoint: PiControlEndpoint,
    session_id: String,
    _root: tempfile::TempDir,
}

impl PiProcess {
    async fn start(session_id: &str, runtime_id: &str) -> Self {
        let root = tempfile::Builder::new()
            .prefix("pc-")
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../clients/pi/test/control-server.mjs");
        let mut child = Command::new("node")
            .arg(fixture)
            .arg(root.path())
            .args([session_id, runtime_id])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut path = String::new();
        tokio::time::timeout(
            Duration::from_secs(10),
            BufReader::new(child.stdout.take().unwrap()).read_line(&mut path),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(!path.is_empty());
        Self {
            child,
            endpoint: PiControlEndpoint {
                runtime_instance_id: runtime_id.into(),
                socket_path: path.trim().into(),
                version: pontia_runtime::pi_control::PROTOCOL_VERSION,
            },
            session_id: session_id.into(),
            _root: root,
        }
    }

    async fn stop(&mut self) {
        self.child.stdin.take();
        assert!(self.child.wait().await.unwrap().success());
    }

    fn controller(&self) -> PiControlConnection {
        PiControlConnection::new(self.session_id.clone(), self.endpoint.clone()).unwrap()
    }
}

async fn state() -> (AppState, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("state")).unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("test.db").display()
    ))
    .await
    .unwrap();
    run_migrations(&pool).await.unwrap();
    (AppState::builder(pool, root.path().into()).build(), root)
}

async fn bind(state: &AppState, pi: &PiProcess) {
    sqlx::query("INSERT INTO sessions (session_id, client_type, state) VALUES (?, 'pi', 'idle') ON CONFLICT DO NOTHING")
        .bind(&pi.session_id).execute(&state.db()).await.unwrap();
    sqlx::query("INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, binding_state) VALUES (?, 'tmux', ?, 'confirmed') ON CONFLICT(session_id) DO UPDATE SET runtime_instance_id = excluded.runtime_instance_id")
        .bind(&pi.session_id).bind(&pi.endpoint.runtime_instance_id).execute(&state.db()).await.unwrap();
}

async fn publish(state: &AppState, pi: &PiProcess) -> StatusCode {
    let request = Request::builder()
        .method("POST")
        .uri("/internal/v1/runtime-bindings/pi-control")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "session_id": pi.session_id, "runtime_instance_id": pi.endpoint.runtime_instance_id,
                "socket_path": pi.endpoint.socket_path, "version": pi.endpoint.version,
            })
            .to_string(),
        ))
        .unwrap();
    pontia_http::router(state.clone())
        .oneshot(request)
        .await
        .unwrap()
        .status()
}

async fn eventually_ping(connection: &PiControlConnection) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if connection.ping().await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn submissions_connect_on_demand_reuse_and_reconnect_only_for_the_next_request() {
    use pontia_application::PublishPiControlEndpoint;
    use serde_json::Value;
    use tokio::{io::AsyncWriteExt, net::UnixListener};

    let (state, root) = state().await;
    let path = root.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    sqlx::query(
        "INSERT INTO sessions (session_id, client_type, state) VALUES ('sess_pi', 'pi', 'idle')",
    )
    .execute(&state.db())
    .await
    .unwrap();
    sqlx::query("INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, binding_state) VALUES ('sess_pi', 'tmux', 'rtinst_pi', 'confirmed')")
        .execute(&state.db()).await.unwrap();
    let service = state.pi_control();
    service
        .publish_endpoint(PublishPiControlEndpoint {
            session_id: "sess_pi".into(),
            runtime_instance_id: "rtinst_pi".into(),
            socket_path: path.display().to_string(),
            version: pontia_runtime::pi_control::PROTOCOL_VERSION,
        })
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err(),
        "publishing an endpoint must not open a connection"
    );

    let server = tokio::spawn(async move {
        for inputs in [vec!["first", "uncertain"], vec!["next"]] {
            let (socket, _) = listener.accept().await.unwrap();
            let mut stream = BufReader::new(socket);
            let mut line = String::new();
            stream.read_line(&mut line).await.unwrap();
            let hello: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(hello["method"], "hello");
            assert_eq!(hello["id"], 0);
            let reply = json!({"jsonrpc":"2.0","id":hello["id"],"result":{"session_id":"sess_pi","runtime_instance_id":"rtinst_pi"}});
            stream
                .get_mut()
                .write_all(format!("{reply}\n").as_bytes())
                .await
                .unwrap();
            for input in inputs {
                line.clear();
                stream.read_line(&mut line).await.unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                assert_eq!(request["method"], "submit");
                assert_eq!(request["params"]["input"], input);
                if input == "uncertain" {
                    break;
                }
                let reply = json!({"jsonrpc":"2.0","id":request["id"],"result":{"accepted":true}});
                stream
                    .get_mut()
                    .write_all(format!("{reply}\n").as_bytes())
                    .await
                    .unwrap();
            }
        }
    });
    tokio::time::timeout(Duration::from_secs(3), async {
        service
            .submit("sess_pi", "rtinst_pi", "first", None)
            .await
            .unwrap();
        assert!(
            service
                .submit("sess_pi", "rtinst_pi", "uncertain", None)
                .await
                .is_err()
        );
        service
            .submit("sess_pi", "rtinst_pi", "next", None)
            .await
            .unwrap();
        server.await.unwrap();
    })
    .await
    .unwrap();
    service.close().await;
}

#[tokio::test]
async fn routes_registered_endpoints_fences_replacements_and_recovers_from_daemon_restart() {
    let (state, _root) = state().await;
    let service = state.pi_control();
    let mut first = PiProcess::start("sess_one", "rtinst_one").await;
    let mut other = PiProcess::start("sess_two", "rtinst_two").await;
    for pi in [&first, &other] {
        assert_eq!(publish(&state, pi).await, StatusCode::CONFLICT);
        bind(&state, pi).await;
        sqlx::query(
            "UPDATE runtime_bindings SET binding_state = 'provisioned' WHERE session_id = ?",
        )
        .bind(&pi.session_id)
        .execute(&state.db())
        .await
        .unwrap();
        assert_eq!(publish(&state, pi).await, StatusCode::CONFLICT);
        sqlx::query("UPDATE runtime_bindings SET binding_state = 'confirmed' WHERE session_id = ?")
            .bind(&pi.session_id)
            .execute(&state.db())
            .await
            .unwrap();
        assert!(
            service
                .ping(&pi.session_id, &pi.endpoint.runtime_instance_id)
                .await
                .is_err(),
            "registration alone provides no control endpoint"
        );
        assert_eq!(publish(&state, pi).await, StatusCode::OK);
    }
    let (a, b) = tokio::join!(
        service.ping("sess_one", "rtinst_one"),
        service.ping("sess_two", "rtinst_two")
    );
    a.unwrap();
    b.unwrap();
    assert!(
        first
            .controller()
            .ping()
            .await
            .unwrap_err()
            .to_string()
            .contains("connection_busy")
    );
    state
        .with_external_api_token(Some("token".into()))
        .pi_control()
        .ping("sess_one", "rtinst_one")
        .await
        .unwrap();

    let mut replacement = PiProcess::start("sess_one", "rtinst_new").await;
    bind(&state, &replacement).await;
    assert_eq!(publish(&state, &first).await, StatusCode::CONFLICT);
    assert!(service.ping("sess_one", "rtinst_one").await.is_err());
    assert_eq!(publish(&state, &replacement).await, StatusCode::OK);
    service.ping("sess_one", "rtinst_new").await.unwrap();
    let released = first.controller();
    eventually_ping(&released).await;
    released.invalidate();

    service.close().await;
    let restarted = AppState::builder(state.db(), state.pontia_home().into()).build();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if restarted
                .pi_control()
                .ping("sess_one", "rtinst_new")
                .await
                .is_ok()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    restarted
        .pi_control()
        .ping("sess_two", "rtinst_two")
        .await
        .unwrap();
    replacement.stop().await;
    assert!(
        restarted
            .pi_control()
            .ping("sess_one", "rtinst_new")
            .await
            .is_err()
    );
    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert_eq!(
        events, 0,
        "transport success and failure must not synthesize lifecycle facts"
    );
    let states: Vec<String> = sqlx::query_scalar("SELECT state FROM sessions")
        .fetch_all(&state.db())
        .await
        .unwrap();
    assert_eq!(states, ["idle", "idle"]);
    restarted.pi_control().close().await;
    first.stop().await;
    other.stop().await;
}

#[tokio::test]
async fn exit_events_release_bindings_and_shutdown_closes_connections() {
    let (state, _root) = state().await;
    let mut pi = PiProcess::start("sess_pi", "rtinst_pi").await;
    bind(&state, &pi).await;
    assert_eq!(publish(&state, &pi).await, StatusCode::OK);
    let service = state.pi_control();
    service.ping("sess_pi", "rtinst_pi").await.unwrap();
    let observer = pi.controller();
    pontia_application::EventIngestService::new(state.db())
        .with_pi_control(service.clone())
        .ingest_reported_event(pontia_core::domain::ReportedEvent::new(
            "evt_exit".into(),
            "sess_pi".into(),
            None,
            pontia_core::domain::EventSource::AgentClient,
            "pi".into(),
            pontia_core::domain::EventType::SessionExited,
            json!({"runtime_instance_id":"rtinst_pi"}),
        ))
        .await
        .unwrap();
    eventually_ping(&observer).await;
    observer.invalidate();
    assert_eq!(publish(&state, &pi).await, StatusCode::CONFLICT);
    sqlx::query("UPDATE sessions SET state = 'idle' WHERE session_id = 'sess_pi'")
        .execute(&state.db())
        .await
        .unwrap();
    assert_eq!(publish(&state, &pi).await, StatusCode::OK);
    // Wait for the peer to release the previous diagnostic connection.
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if service.ping("sess_pi", "rtinst_pi").await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    service.close().await;
    assert!(
        state
            .pi_control()
            .ping("sess_pi", "rtinst_pi")
            .await
            .is_err()
    );
    let after_shutdown = pi.controller();
    eventually_ping(&after_shutdown).await;
    after_shutdown.invalidate();
    pi.stop().await;
}

#[tokio::test]
async fn session_errors_and_process_exit_observations_release_idle_connections() {
    for process_exit in [false, true] {
        let (state, _root) = state().await;
        let mut pi = PiProcess::start("sess_pi", "rtinst_pi").await;
        bind(&state, &pi).await;
        assert_eq!(publish(&state, &pi).await, StatusCode::OK);
        let service = state.pi_control();
        service.ping("sess_pi", "rtinst_pi").await.unwrap();
        if process_exit {
            sqlx::query("UPDATE runtime_bindings SET tmux_socket_path='/unused/tmux', tmux_pane_id='%1' WHERE session_id='sess_pi'")
                .execute(&state.db()).await.unwrap();
            pontia_application::RuntimeObservationService::new(state.event_ingest_service())
                .observe_session("sess_pi")
                .await
                .unwrap();
        } else {
            pontia_application::EventIngestService::new(state.db())
                .with_pi_control(service.clone())
                .ingest_reported_event(pontia_core::domain::ReportedEvent::new(
                    "evt_error".into(), "sess_pi".into(), None,
                    pontia_core::domain::EventSource::AgentClient, "pi".into(),
                    pontia_core::domain::EventType::SessionError,
                    json!({"runtime_instance_id":"rtinst_pi", "failure":{"message":"client error"}}),
                )).await.unwrap();
        }
        let observer = pi.controller();
        eventually_ping(&observer).await;
        observer.invalidate();
        service.close().await;
        pi.stop().await;
    }
}

#[tokio::test]
async fn external_inbox_uses_the_registered_connection_without_paste_or_turn_facts() {
    use http_body_util::BodyExt;
    use pontia_application::EventIngestService;
    let (state, _root) = state().await;
    let state = state.with_external_api_token(Some("token".into()));
    let mut pi = PiProcess::start("sess_input", "rtinst_input").await;
    bind(&state, &pi).await;
    sqlx::query("UPDATE runtime_bindings SET tmux_socket_path='/unused/tmux',tmux_pane_id='%1',capabilities=? WHERE session_id='sess_input'")
        .bind(serde_json::to_string(&pontia_agent_clients::pi::CAPABILITIES).unwrap()).execute(&state.db()).await.unwrap();
    assert_eq!(publish(&state, &pi).await, StatusCode::OK);
    EventIngestService::new(state.db())
        .ingest_reported_event(pontia_core::domain::ReportedEvent::new(
            "evt_input_ready".into(),
            "sess_input".into(),
            None,
            pontia_core::domain::EventSource::AgentClient,
            "pi".into(),
            pontia_core::domain::EventType::SessionReady,
            json!({"runtime_instance_id":"rtinst_input"}),
        ))
        .await
        .unwrap();
    let response = pontia_http::router(state.clone())
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
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let message_id = body["data"]["inbox_message"]["message_id"]
        .as_str()
        .unwrap();
    assert_eq!(body["data"]["inbox_message"]["state"], "dispatched");
    let delivered: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(pi._root.path().join("messages.jsonl")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        delivered,
        json!({"input":"first\n你好", "inboxMessageId":message_id})
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns")
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert_eq!(count, 0);
    let contexts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pending_turn_contexts")
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert_eq!(contexts, 0);
    // A client fact, not the delivery response, creates the turn and links the Inbox.
    EventIngestService::new(state.db()).ingest_reported_event(pontia_core::domain::ReportedEvent::new(
        "evt_input_started".into(), "sess_input".into(), Some("turn_input".into()),
        pontia_core::domain::EventSource::AgentClient, "pi".into(), pontia_core::domain::EventType::TurnStarted,
        json!({"runtime_instance_id":"rtinst_input", "input":{"summary":"first\n你好"}, "metadata":{"inbox_message_id":message_id}}),
    )).await.unwrap();
    let turn_id: Option<String> =
        sqlx::query_scalar("SELECT turn_id FROM inbox_messages WHERE message_id=?")
            .bind(message_id)
            .fetch_one(&state.db())
            .await
            .unwrap();
    assert_eq!(turn_id.as_deref(), Some("turn_input"));
    // Disconnects do not synthesize execution failure or replay input through tmux.
    state.pi_control().close().await;
    pi.stop().await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_type IN ('turn.failed','turn.completed','session.exited')").fetch_one(&state.db()).await.unwrap();
    assert_eq!(count, 0);
}
