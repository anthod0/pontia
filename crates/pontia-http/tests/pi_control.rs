use pontia_client_pi::ipc::PiIpcListener;
mod common;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use pontia_application::{AppState, EventIngestService};
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
    let state = AppState::builder(pool, root.path().into())
        .clients(crate::common::clients::clients())
        .external_api_token(Some("token".into()))
        .build();
    (state, root)
}

async fn bind(state: &AppState, session: &str, runtime: &str) {
    sqlx::query("INSERT INTO sessions (session_id,client_type,state) VALUES (?,'pi','idle') ON CONFLICT DO NOTHING").bind(session).execute(&state.db()).await.unwrap();
    sqlx::query("INSERT INTO runtime_bindings (session_id,runtime_kind,runtime_instance_id,binding_state,tmux_socket_path,tmux_pane_id,capabilities) VALUES (?,'tmux',?,'confirmed','/unused/tmux','%1',?) ON CONFLICT(session_id) DO UPDATE SET runtime_instance_id=excluded.runtime_instance_id")
        .bind(session).bind(runtime).bind(serde_json::to_string(&pontia_client_pi::CAPABILITIES).unwrap()).execute(&state.db()).await.unwrap();
    sqlx::query("INSERT INTO agent_bindings (id,session_id,client_type,launch_cwd,client_session_key,metadata) VALUES (?,?,'pi','/unused',?,'{}') ON CONFLICT DO NOTHING")
        .bind(format!("binding_{session}")).bind(session).bind(format!("native_{session}")).execute(&state.db()).await.unwrap();
}

async fn attach(
    state: &AppState,
    session: &str,
    runtime: &str,
) -> (
    std::sync::Arc<PiRpcPeer>,
    tokio::sync::mpsc::Receiver<pontia_client_pi::rpc::RpcRequest>,
) {
    let (server, client) = UnixStream::pair().unwrap();
    let (peer, requests) = PiRpcPeer::new(server);
    state
        .client_control()
        .attach(
            "pi",
            session,
            runtime,
            &format!("native_{session}"),
            peer.clone(),
        )
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
        let control = state.client_control();
        tokio::spawn(async move { control.ping("sess_pi", "rt_old").await })
    };
    let request = requests.recv().await.unwrap();
    old.reply(request.id, json!({"pong":true})).await.unwrap();
    ping.await.unwrap().unwrap();
    let blocked = {
        let control = state.client_control();
        tokio::spawn(async move { control.submit("sess_pi", "rt_old", "uncertain", None).await })
    };
    requests.recv().await.unwrap();
    bind(&state, "sess_pi", "rt_new").await;
    let (new, _requests) = attach(&state, "sess_pi", "rt_new").await;
    assert!(matches!(
        blocked.await.unwrap(),
        Err(pontia_core::Error::ControlUnknown(_))
    ));
    assert!(
        state
            .client_control()
            .ping("sess_pi", "rt_old")
            .await
            .is_err()
    );
    state
        .event_ingest_service()
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
    assert!(!state.client_control().available("sess_pi").await.unwrap());
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
        .client_control()
        .ping("sess_input", "rt_input")
        .await
        .unwrap();
    state.shutdown().notify();
    task.await.unwrap().unwrap();
    state.client_control().close().await;
    let restarted = AppState::builder(state.db(), root.path().into())
        .clients(crate::common::clients::clients())
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
            json!({"runtime_instance_id":"rt_input"}),
        ))
        .await
        .unwrap();
    let response = pontia_http::router(restarted.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/sessions/sess_input/inbox/messages")
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
    restarted.client_control().close().await;
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
async fn pi_models_use_the_bound_client_and_only_reported_facts_update_the_current_model() {
    let (state, _root) = state().await;
    bind(&state, "sess_models", "rt_models").await;
    let (client, mut requests) = attach(&state, "sess_models", "rt_models").await;
    let catalog = json!({"models":[
        {"id":"one/shared", "name":"First", "description":"one"},
        {"id":"two/shared", "name":"Second", "description":"two"}
    ]});
    let task = {
        let state = state.clone();
        tokio::spawn(async move { control_request(&state, "GET", "models", Value::Null).await })
    };
    let request = requests.recv().await.unwrap();
    assert_eq!(request.method, "models.list");
    client.reply(request.id, catalog.clone()).await.unwrap();
    let (status, body) = task.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["models"], catalog["models"]);
    assert_eq!(body["data"]["runtime_instance_id"], "rt_models");
    let change = json!({"model":"two/shared", "runtime_instance_id":"rt_models"});
    let task = {
        let state = state.clone();
        let change = change.clone();
        tokio::spawn(async move { control_request(&state, "PATCH", "model", change).await })
    };
    let request = requests.recv().await.unwrap();
    assert_eq!(request.method, "model.set");
    assert_eq!(request.params, json!({"model":"two/shared"}));
    client
        .reply(request.id, json!({"accepted":true}))
        .await
        .unwrap();
    assert!(task.await.unwrap().0.is_success());
    let query = pontia_application::ExternalQueryService::new(state.db());
    assert_eq!(
        query
            .get_session("sess_models")
            .await
            .unwrap()
            .unwrap()
            .model,
        None
    );
    state
        .event_ingest_service()
        .report_fact(pontia_application::ReportedFact {
            session_id: "sess_models".into(),
            turn_id: None,
            fact_type: pontia_core::domain::EventType::SessionModelUpdated,
            data: json!({"model":"two/shared", "runtime_instance_id":"rt_models"}),
        })
        .await
        .unwrap();
    assert_eq!(
        query
            .get_session("sess_models")
            .await
            .unwrap()
            .unwrap()
            .model
            .as_deref(),
        Some("two/shared")
    );
    let stale = json!({"model":"one/shared", "runtime_instance_id":"rt_old"});
    assert_eq!(
        control_request(&state, "PATCH", "model", stale).await.0,
        StatusCode::CONFLICT
    );
    assert!(requests.try_recv().is_err());
    let task = {
        let state = state.clone();
        tokio::spawn(async move { control_request(&state, "PATCH", "model", change).await })
    };
    let request = requests.recv().await.unwrap();
    client
        .reply_error(request.id, -32006, "Model authentication failed")
        .await
        .unwrap();
    assert!(!task.await.unwrap().0.is_success());
    assert_eq!(
        query
            .get_session("sess_models")
            .await
            .unwrap()
            .unwrap()
            .model
            .as_deref(),
        Some("two/shared")
    );
    state.client_control().close().await;
    assert!(
        !control_request(&state, "GET", "models", Value::Null)
            .await
            .0
            .is_success()
    );
}

#[tokio::test]
async fn model_requests_reject_malformed_catalogs_and_runtime_replacement() {
    let (state, _root) = state().await;
    bind(&state, "sess_models", "rt_models").await;
    let (client, mut requests) = attach(&state, "sess_models", "rt_models").await;
    let task = {
        let control = state.client_control();
        tokio::spawn(async move {
            control
                .set_model("sess_models", "rt_models", "one/shared")
                .await
        })
    };
    let request = requests.recv().await.unwrap();
    client
        .reply_error(request.id, -32007, "Observation acknowledgement lost")
        .await
        .unwrap();
    assert!(matches!(
        task.await.unwrap(),
        Err(pontia_core::Error::ControlUnknown(_))
    ));
    let task = {
        let control = state.client_control();
        tokio::spawn(async move { control.list_models("sess_models", "rt_models").await })
    };
    let request = requests.recv().await.unwrap();
    let model = json!({"id":"one/shared", "name":"Model", "description":"one"});
    client
        .reply(request.id, json!({"models":[model, model]}))
        .await
        .unwrap();
    assert!(matches!(
        task.await.unwrap(),
        Err(pontia_core::Error::ControlUnknown(_))
    ));
    let task = {
        let control = state.client_control();
        tokio::spawn(async move {
            control
                .set_model("sess_models", "rt_models", "one/shared")
                .await
        })
    };
    requests.recv().await.unwrap();
    bind(&state, "sess_models", "rt_new").await;
    let (_new, _requests) = attach(&state, "sess_models", "rt_new").await;
    assert!(matches!(
        task.await.unwrap(),
        Err(pontia_core::Error::ControlUnknown(_))
    ));
    state.client_control().close().await;
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
        ("DELETE", "", "shutdown"),
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
    let query = pontia_application::ExternalQueryService::new(state.db());
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
    assert!(
        state
            .client_control()
            .interrupt("sess_models", "rt_old")
            .await
            .is_err()
    );
    assert!(
        state
            .client_control()
            .shutdown("sess_models", "rt_old")
            .await
            .is_err()
    );
    assert!(requests.try_recv().is_err());
    state.client_control().close().await;
    for (method, resource) in [("POST", "interrupt"), ("DELETE", "")] {
        let (status, body) = control_request(&state, method, resource, Value::Null).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(body["error"]["code"], "capability_unavailable");
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
                let control = state.client_control();
                tokio::spawn(async move {
                    if method == "interrupt" {
                        control.interrupt("sess_control", "rt_control").await
                    } else {
                        control.shutdown("sess_control", "rt_control").await
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
                assert!(matches!(error, pontia_core::Error::Domain(_)));
            } else {
                assert!(matches!(error, pontia_core::Error::ControlUnknown(_)));
            }
        }
    }
}

#[tokio::test]
async fn shutdown_accepts_its_own_exit_but_not_a_replacement_instances_exit() {
    for runtime in ["rt_control", "rt_replacement"] {
        let (state, _root) = state().await;
        bind(&state, "sess_control", "rt_control").await;
        let (client, mut requests) = attach(&state, "sess_control", "rt_control").await;
        let task = {
            let control = state.client_control();
            tokio::spawn(async move { control.shutdown("sess_control", "rt_control").await })
        };
        let request = requests.recv().await.unwrap();
        // Commit the exit before the shutdown caller can perform its post-reply check.
        // Keep the socket open here to deliver the native acknowledgement separately.
        bind(&state, "sess_control", runtime).await;
        EventIngestService::for_projection_tests(state.db())
            .with_clients(crate::common::clients::clients())
            .ingest_reported_event(pontia_core::domain::ReportedEvent::new(
                "evt_shutdown".into(),
                "sess_control".into(),
                None,
                pontia_core::domain::EventSource::AgentClient,
                "pi".into(),
                pontia_core::domain::EventType::SessionExited,
                json!({"runtime_instance_id":runtime}),
            ))
            .await
            .unwrap();
        client
            .reply(request.id, json!({"accepted":true}))
            .await
            .unwrap();
        let result = task.await.unwrap();
        if runtime == "rt_control" {
            result.unwrap();
        } else {
            assert!(matches!(result, Err(pontia_core::Error::ControlUnknown(_))));
        }
        state.client_control().close().await;
    }
}
