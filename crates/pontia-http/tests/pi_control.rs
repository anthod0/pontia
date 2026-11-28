use pontia_agent_control::ipc::ControlListener;
mod common;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use pontia_agent_control::EventIngestService;
use pontia_application::AppState;
use pontia_client_pi::rpc::PiRpcPeer;
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
    pontia_agent_control::workspaces::upsert_canonical_workspace(
        &pool,
        root.path().canonicalize().unwrap().to_str().unwrap(),
        None,
    )
    .await
    .unwrap();
    let state = AppState::builder(
        pontia_agent_control::AgentControl::builder(pool, root.path().into())
            .clients(crate::common::clients::clients())
            .build(),
    )
    .external_api_token(Some("token".into()))
    .build();
    (state, root)
}

async fn bind(state: &AppState, session: &str, runtime: &str) {
    sqlx::query("INSERT INTO sessions (session_id,client_type,state) VALUES (?,'pi','idle') ON CONFLICT DO NOTHING").bind(session).execute(&state.db()).await.unwrap();
    sqlx::query("UPDATE session_runtimes SET runtime_id=? WHERE session_id=?")
        .bind(runtime)
        .bind(session)
        .execute(&state.db())
        .await
        .unwrap();
    sqlx::query("INSERT INTO session_runtimes(runtime_id,session_id,role,state,tmux_socket_path,tmux_pane_id,created_at) VALUES (?,?,'pi_tui','running','/unused/tmux','%1',strftime('%Y-%m-%dT%H:%M:%fZ','now')) ON CONFLICT(runtime_id) DO NOTHING")
        .bind(runtime).bind(session).execute(&state.db()).await.unwrap();
    sqlx::query("INSERT INTO agent_bindings (id,session_id,client_type,launch_cwd,client_session_key,metadata) VALUES (?,?,'pi','/unused',?,'{}') ON CONFLICT DO NOTHING")
        .bind(format!("binding_{session}")).bind(session).bind(format!("native_{session}")).execute(&state.db()).await.unwrap();
}

async fn attach(
    state: &AppState,
    session: &str,
    runtime: &str,
) -> (
    std::sync::Arc<PiRpcPeer>,
    tokio::sync::mpsc::Receiver<pontia_agent_control::ipc::rpc::RpcRequest>,
) {
    let fingerprint = pontia_runtime::ProcessFingerprint::capture(std::process::id()).unwrap();
    sqlx::query("UPDATE session_runtimes SET process_fingerprint=? WHERE runtime_id=?")
        .bind(serde_json::to_string(&fingerprint).unwrap())
        .bind(runtime)
        .execute(&state.db())
        .await
        .unwrap();
    let (server, client) = UnixStream::pair().unwrap();
    tokio::spawn(pontia_client_pi::ipc::serve_connection(
        state.control().clone(),
        server,
    ));
    let (client, requests) = PiRpcPeer::new(client);
    client
        .call(
            "runtime.attach",
            json!({
                "version": pontia_agent_control::ipc::rpc::PROTOCOL_VERSION,
                "session_id": session,
                "runtime_id": runtime,
                "client_session_key": format!("native_{session}"),
            }),
        )
        .await
        .unwrap();
    (client, requests)
}

fn session_client(
    state: &AppState,
) -> std::sync::Arc<dyn pontia_agent_control::client_contract::ClientSession> {
    state
        .control()
        .clients()
        .get("pi")
        .unwrap()
        .session
        .clone()
        .unwrap()
}

async fn target(state: &AppState, session: &str) -> pontia_agent_control::runtime::ControlTarget {
    pontia_agent_control::runtime::ControlTarget::resolve(
        &state.db(),
        &state.control().clients(),
        session,
        None,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn replacement_and_exit_fence_connections_without_synthesizing_facts() {
    let (state, _root) = state().await;
    bind(&state, "sess_pi", "rt_old").await;
    let (old, mut requests) = attach(&state, "sess_pi", "rt_old").await;
    let control = {
        let client = session_client(&state);
        let events = state.control().event_ingest_service();
        let target = target(&state, "sess_pi").await;
        tokio::spawn(async move { client.set_reasoning_effort(events, &target, "high").await })
    };
    let request = requests.recv().await.unwrap();
    old.reply(request.id, json!({"accepted":true}))
        .await
        .unwrap();
    control.await.unwrap().unwrap();
    let blocked = {
        let client = session_client(&state);
        let events = state.control().event_ingest_service();
        let target = target(&state, "sess_pi").await;
        tokio::spawn(async move {
            let payload = pontia_agent_control::InputPayload::from("uncertain");
            client.start_turn(events, &target, &payload, None).await
        })
    };
    requests.recv().await.unwrap();
    old.close();
    let (new, _requests) = attach(&state, "sess_pi", "rt_old").await;
    assert!(matches!(
        blocked.await.unwrap(),
        Err(pontia_agent_control::DeliveryError::Uncertain(_))
    ));
    let stale = pontia_agent_control::runtime::ControlTarget {
        session_id: "sess_pi".into(),
        runtime_id: Some("unrelated".into()),
        control_runtime_role: Some("pi_tui"),
    };
    assert!(
        session_client(&state)
            .set_reasoning_effort(state.control().event_ingest_service(), &stale, "high",)
            .await
            .is_err()
    );
    new.call(
        "event.report",
        json!({
            "runtime_id":"rt_old",
            "event":{
                "session_id":"sess_pi",
                "turn_id":null,
                "report_key":"evt_exit",
                "type":"session.exited",
                "data":{"reason":"test"}
            }
        }),
    )
    .await
    .unwrap();
    assert_eq!(
        state
            .control()
            .queries()
            .get_session("sess_pi")
            .await
            .unwrap()
            .unwrap()
            .control_connected,
        Some(false)
    );
    tokio::time::timeout(Duration::from_secs(1), new.closed())
        .await
        .unwrap();
    assert_eq!(
        state
            .control()
            .queries()
            .get_session("sess_pi")
            .await
            .unwrap()
            .unwrap()
            .control_connected,
        Some(false)
    );
    let event_types: Vec<String> =
        sqlx::query_scalar("SELECT event_type FROM events ORDER BY rowid")
            .fetch_all(&state.db())
            .await
            .unwrap();
    assert_eq!(event_types, vec!["session.exited", "runtime.exited"]);
}

#[tokio::test]
async fn real_pi_client_reconnects_after_daemon_restart_and_delivers_external_input() {
    let (state, root) = state().await;
    bind(&state, "sess_input", "rt_input").await;
    sqlx::query("UPDATE sessions SET state='starting' WHERE session_id='sess_input'")
        .execute(&state.db())
        .await
        .unwrap();
    let listener = ControlListener::bind(root.path()).await.unwrap();
    let task = tokio::spawn(listener.run(
        state.control().clone(),
        state.control().shutdown().subscribe(),
    ));
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
    assert_eq!(
        state
            .control()
            .queries()
            .get_session("sess_input")
            .await
            .unwrap()
            .unwrap()
            .control_connected,
        Some(true)
    );
    state.control().shutdown().notify();
    task.await.unwrap().unwrap();
    let restarted = AppState::builder(
        pontia_agent_control::AgentControl::builder(state.db(), root.path().into())
            .clients(crate::common::clients::clients())
            .build(),
    )
    .external_api_token(Some("token".into()))
    .build();
    EventIngestService::for_projection_tests(restarted.db())
        .with_clients(crate::common::clients::clients())
        .ingest_reported_event(pontia_core::domain::ReportedEvent::new(
            "evt_ready".into(),
            "sess_input".into(),
            None,
            pontia_core::domain::EventSource::AgentClient,
            "pi".into(),
            pontia_core::domain::EventType::SessionReady,
            json!({"runtime_id":"rt_input"}),
        ))
        .await
        .unwrap();
    let response = pontia_http::router(restarted.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/sessions/sess_input/messages")
                .header("content-type", "application/json")
                .header("authorization", "Bearer token")
                .body(Body::from(
                    json!({"input":"first\n你好","mode":"queue"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["delivery"]["status"], "queued");
    let message_id = body["data"]["delivery"]["message_id"]
        .as_str()
        .unwrap()
        .to_string();
    let listener = ControlListener::bind(root.path()).await.unwrap();
    let task = tokio::spawn(listener.run(
        restarted.control().clone(),
        restarted.control().shutdown().subscribe(),
    ));
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let remaining: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM inbox_messages WHERE message_id=?")
                    .bind(&message_id)
                    .fetch_one(&state.db())
                    .await
                    .unwrap();
            if remaining == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        restarted
            .control()
            .queries()
            .get_session("sess_input")
            .await
            .unwrap()
            .unwrap()
            .control_connected,
        Some(true)
    );
    let delivered: Value =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("messages.jsonl")).unwrap())
            .unwrap();
    assert_eq!(
        delivered,
        json!({"input":"first\n你好","mode":"start_turn"})
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
    restarted.control().shutdown().notify();
    task.await.unwrap().unwrap();
}

async fn control_request(
    state: &AppState,
    method: &str,
    resource: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = pontia_http::router(state.clone())
        .oneshot(
            Request::builder()
                .method(method)
                .uri(format!("/api/v1/sessions/sess_models/{resource}").trim_end_matches('/'))
                .header("content-type", "application/json")
                .header("authorization", "Bearer token")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    (
        response.status(),
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap(),
    )
}

#[tokio::test]
async fn pi_interrupt_and_shutdown_use_rpc_and_wait_for_client_lifecycle_facts() {
    let (state, _root) = state().await;
    bind(&state, "sess_models", "rt_models").await;
    sqlx::query("UPDATE sessions SET state='busy' WHERE session_id='sess_models'")
        .execute(&state.db())
        .await
        .unwrap();
    sqlx::query("INSERT INTO turns(turn_id,session_id,state) VALUES ('turn_control','sess_models','running')")
        .execute(&state.db()).await.unwrap();
    let (client, mut requests) = attach(&state, "sess_models", "rt_models").await;
    for (method, resource, rpc_method) in [
        ("POST", "interrupt", "interrupt"),
        ("POST", "exit", "shutdown"),
    ] {
        let task = {
            let state = state.clone();
            tokio::spawn(
                async move { control_request(&state, method, resource, Value::Null).await },
            )
        };
        let request = tokio::time::timeout(Duration::from_secs(2), requests.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(request.method, rpc_method);
        assert_eq!(request.params, json!({}));
        client
            .reply(request.id, json!({"accepted":true}))
            .await
            .unwrap();
        let (status, body) = task.await.unwrap();
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let query = pontia_agent_control::ControlQueryService::new(state.db());
    assert_eq!(
        query
            .get_session("sess_models")
            .await
            .unwrap()
            .unwrap()
            .state,
        "busy"
    );
    assert_eq!(
        query
            .get_turn("sess_models", "turn_control")
            .await
            .unwrap()
            .unwrap()
            .state,
        "running"
    );
    let facts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE event_type IN ('turn.interrupted','session.exited')",
    )
    .fetch_one(&state.db())
    .await
    .unwrap();
    assert_eq!(facts, 0);
    let stale = pontia_agent_control::runtime::ControlTarget {
        session_id: "sess_models".into(),
        runtime_id: Some("rt_old".into()),
        control_runtime_role: Some("pi_tui"),
    };
    assert!(
        session_client(&state)
            .interrupt(state.control().event_ingest_service(), &stale)
            .await
            .is_err()
    );
    assert!(
        session_client(&state)
            .exit(
                state.control().event_ingest_service(),
                &stale,
                pontia_agent_control::client_contract::ClientExitMode::Unconditional,
            )
            .await
            .is_err()
    );
    assert!(requests.try_recv().is_err());
    client.close();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let connected = state
                .control()
                .queries()
                .get_session("sess_models")
                .await
                .unwrap()
                .unwrap()
                .control_connected;
            if connected == Some(false) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    sqlx::query(
        "UPDATE session_runtimes SET process_fingerprint=NULL WHERE runtime_id='rt_models'",
    )
    .execute(&state.db())
    .await
    .unwrap();
    for (method, resource, expected_status, code) in [
        (
            "POST",
            "interrupt",
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
        ),
        (
            "POST",
            "exit",
            StatusCode::UNPROCESSABLE_ENTITY,
            "capability_unavailable",
        ),
    ] {
        let (status, body) = control_request(&state, method, resource, Value::Null).await;
        assert_eq!(status, expected_status, "{body}");
        assert_eq!(body["error"]["code"], code);
    }
}

#[tokio::test]
async fn pi_lifecycle_controls_preserve_rejection_and_unknown_delivery() {
    for method in ["interrupt", "shutdown"] {
        let (state, _root) = state().await;
        bind(&state, "sess_control", "rt_control").await;
        let (client, mut requests) = attach(&state, "sess_control", "rt_control").await;
        for outcome in ["rejected", "malformed", "disconnected"] {
            let task = {
                let client_session = session_client(&state);
                let events = state.control().event_ingest_service();
                let target = target(&state, "sess_control").await;
                tokio::spawn(async move {
                    if method == "interrupt" {
                        client_session
                            .interrupt(events, &target)
                            .await
                            .map_err(pontia_core::Error::from)
                    } else {
                        client_session
                            .exit(
                                events,
                                &target,
                                pontia_agent_control::client_contract::ClientExitMode::Unconditional,
                            )
                            .await
                            .map(|_| ())
                    }
                })
            };
            let request = requests.recv().await.unwrap();
            assert_eq!(request.method, method);
            match outcome {
                "rejected" => client
                    .reply_error(request.id, -32006, "Session is no longer current")
                    .await
                    .unwrap(),
                "malformed" => client
                    .reply(request.id, json!({"accepted": false}))
                    .await
                    .unwrap(),
                _ => client.close(),
            }
            let error = task.await.unwrap().unwrap_err();
            if outcome == "rejected" {
                if method == "interrupt" {
                    assert!(matches!(error, pontia_core::Error::Unavailable(_)));
                } else {
                    assert!(matches!(error, pontia_core::Error::Domain(_)));
                }
            } else {
                assert!(matches!(error, pontia_core::Error::ControlUnknown(_)));
            }
        }
    }
}

#[tokio::test]
async fn shutdown_accepts_an_acknowledgement_but_not_one_from_a_replaced_connection() {
    for replaced in [false, true] {
        let (state, _root) = state().await;
        bind(&state, "sess_control", "rt_control").await;
        let (client, mut requests) = attach(&state, "sess_control", "rt_control").await;
        let task = {
            let client_session = session_client(&state);
            let events = state.control().event_ingest_service();
            let target = target(&state, "sess_control").await;
            tokio::spawn(async move {
                client_session
                    .exit(
                        events,
                        &target,
                        pontia_agent_control::client_contract::ClientExitMode::Unconditional,
                    )
                    .await
            })
        };
        let request = requests.recv().await.unwrap();
        let replacement = if replaced {
            client.close();
            Some(attach(&state, "sess_control", "rt_control").await.0)
        } else {
            client
                .reply(request.id, json!({"accepted":true}))
                .await
                .unwrap();
            None
        };
        let result = task.await.unwrap();
        if replaced {
            assert!(matches!(result, Err(pontia_core::Error::ControlUnknown(_))));
        } else {
            assert_eq!(
                result.unwrap(),
                pontia_agent_control::client_contract::ClientExitOutcome::Requested
            );
        }
        client.close();
        if let Some(replacement) = replacement {
            replacement.close();
        }
    }
}
