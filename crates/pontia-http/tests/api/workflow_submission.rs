use crate::common::http;
use pontia_workflow::repository::{
    CreateWorkflowNodeRecord, CreateWorkflowRecord, SqliteWorkflowRepository,
};
use std::{fs, sync::Arc};

use pontia_agent_control::ipc::rpc::{PROTOCOL_VERSION, RpcRequest};
use pontia_client_pi::rpc::PiRpcPeer;
use tokio::net::UnixStream;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_storage_sqlite::repositories::session_runtimes::{
    SessionRuntimeRecord, SqliteSessionRuntimeRepository,
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::common::test_app::TestApp;

pub(super) async fn seed_running_workflow(
    app: &TestApp,
) -> (Arc<PiRpcPeer>, tokio::sync::mpsc::Receiver<RpcRequest>) {
    let workflows = SqliteWorkflowRepository::new(
        app.db.clone(),
        pontia_agent_control::ControlQueryService::new(app.db.clone()),
    );
    workflows
        .create_workflow(CreateWorkflowRecord {
            initiator_session_id: "origin".into(),
            workflow_id: "wf_http_submit".to_string(),
            title: "HTTP submission".to_string(),
            cwd: app.workspace().path().display().to_string(),
            state: "running".to_string(),
        })
        .await
        .expect("create workflow");
    workflows
        .create_node(CreateWorkflowNodeRecord {
            node_id: "node_http_submit".to_string(),
            workflow_id: "wf_http_submit".to_string(),
            parent_node_id: None,
            phase: "Test Phase".to_string(),
            title: "Writer".to_string(),
            instructions: "Write the handoff".to_string(),
            inputs: "[]".to_string(),
            output: "result.md".to_string(),
            execution_profile_id: None,
        })
        .await
        .expect("create node");
    sqlx::query("INSERT INTO sessions (session_id, client_type, state) VALUES (?, 'pi', 'busy')")
        .bind("sess_http_submit")
        .execute(&app.db)
        .await
        .expect("create session");
    workflows
        .bind_node_session("node_http_submit", "sess_http_submit")
        .await
        .expect("bind node session");
    SqliteSessionRuntimeRepository::new(app.db.clone())
        .upsert_binding(SessionRuntimeRecord {
            session_id: "sess_http_submit".to_string(),
            runtime_id: "rtinst_http_submit".to_string(),
            start_command: None,
            tmux_socket_path: Some("/tmp/fake-pontia-tmux.sock".to_string()),
            tmux_pane_id: Some("%42".to_string()),
            process_fingerprint: Some(
                serde_json::to_string(
                    &pontia_runtime::ProcessFingerprint::capture(std::process::id()).unwrap(),
                )
                .unwrap(),
            ),
            role: "pi_tui".into(),
            state: "running".into(),
            created_at: "2026-10-01T00:00:00Z".into(),
        })
        .await
        .expect("create runtime binding");
    fs::create_dir_all(
        app.pontia_home()
            .path()
            .join("workflows/wf_http_submit/handoff"),
    )
    .expect("create handoff directory");

    sqlx::query("INSERT INTO agent_bindings (id, session_id, client_type, launch_cwd, client_session_key, metadata) VALUES ('binding_http_submit', 'sess_http_submit', 'pi', ?, 'native_http_submit', '{}')")
        .bind(app.workspace().path().display().to_string())
        .execute(&app.db)
        .await
        .expect("bind native Pi session");
    let (server, client) = UnixStream::pair().unwrap();
    tokio::spawn(pontia_client_pi::ipc::serve_connection(
        app.state.control().client_context(),
        server,
    ));
    let (client, requests) = PiRpcPeer::new(client);
    client
        .call(
            "runtime.attach",
            json!({
                "version": PROTOCOL_VERSION,
                "session_id": "sess_http_submit",
                "runtime_id": "rtinst_http_submit",
                "client_session_key": "native_http_submit",
            }),
        )
        .await
        .expect("attach Pi control channel");
    (client, requests)
}

async fn post_node_command(app: &TestApp, path: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/workflow/{path}"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, "Bearer test-token");
    let response = http::router(app.state.clone())
        .oneshot(request.body(Body::from(body.to_string())).expect("request"))
        .await
        .expect("response");
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (
        status,
        serde_json::from_slice(&body).expect("json response"),
    )
}

#[tokio::test]
async fn workflow_submission_accepts_the_node_owned_output_file() {
    let app = TestApp::new().await;
    let (_pi, mut requests) = seed_running_workflow(&app).await;
    fs::write(
        app.pontia_home()
            .path()
            .join("workflows/wf_http_submit/handoff/result.md"),
        "Submitted through HTTP: 完成\n",
    )
    .expect("write Agent output");

    let (status, body) = post_node_command(
        &app,
        "submissions",
        json!({
            "session_id": "sess_http_submit",
            "runtime_id": "rtinst_http_submit"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({ "data": { "submitted": true }, "meta": {}, "error": null })
    );
    assert_eq!(
        fs::read_to_string(
            app.pontia_home()
                .path()
                .join("workflows/wf_http_submit/handoff/result.md")
        )
        .expect("read handoff"),
        "Submitted through HTTP: 完成\n"
    );
    let workflows = SqliteWorkflowRepository::new(
        app.db.clone(),
        pontia_agent_control::ControlQueryService::new(app.db.clone()),
    );
    let workflow = workflows
        .get_workflow("wf_http_submit")
        .await
        .expect("load workflow")
        .expect("workflow exists");
    assert_eq!(workflow.state, "running");
    let node = workflows
        .get_node("node_http_submit")
        .await
        .expect("load node")
        .expect("node exists");
    assert!(node.submitted_at.is_some());
    assert!(matches!(
        requests.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn workflow_submission_preserves_service_conflicts() {
    let app = TestApp::new().await;
    let (_pi, _requests) = seed_running_workflow(&app).await;

    let (status, body) = post_node_command(
        &app,
        "submissions",
        json!({
            "session_id": "sess_http_submit",
            "runtime_id": "rtinst_http_submit"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "state_conflict");
    assert!(
        body["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("is unavailable")
    );
    assert!(
        !app.pontia_home()
            .path()
            .join("workflows/wf_http_submit/handoff/result.md")
            .exists()
    );
}

#[tokio::test]
async fn workflow_termination_reports_once_to_the_origin_and_preserves_running_state() {
    let app = TestApp::new().await;
    let (_pi, _requests) = seed_running_workflow(&app).await;
    sqlx::raw_sql(
        "INSERT INTO sessions(session_id,client_type,state) VALUES ('origin','pi','exited');
        INSERT INTO turns(turn_id,session_id,state) VALUES ('turn','sess_http_submit','running');",
    )
    .execute(&app.db)
    .await
    .unwrap();
    let dir = app
        .pontia_home()
        .path()
        .join("workflows/wf_http_submit/nodes/node_http_submit");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("termination.md"),
        "The required source file is missing.",
    )
    .unwrap();
    let mut request = json!({"session_id":"sess_http_submit","runtime_id":"wrong"});
    assert!(
        post_node_command(&app, "terminate", request.clone())
            .await
            .0
            .is_client_error()
    );
    request["runtime_id"] = json!("rtinst_http_submit");
    for _ in 0..2 {
        let (status, body) = post_node_command(&app, "terminate", request.clone()).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let control = app.state.control();
    let coordinator = pontia_workflow::WorkflowCoordinator::new(
        app.db.clone(),
        control,
        control.clone(),
        control.agent_events(),
        app.pontia_home().path().into(),
    );
    let (stop, shutdown) = tokio::sync::watch::channel(false);
    let worker = tokio::spawn(coordinator.run(shutdown));
    let messages = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let messages = control
                .inbox_commands()
                .list_messages("origin")
                .await
                .unwrap();
            if !messages.is_empty() {
                break messages;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    stop.send(true).unwrap();
    worker.await.unwrap();
    assert_eq!(messages.len(), 1);
    for text in [
        "wf_http_submit",
        "node_http_submit",
        "sess_http_submit",
        "The required source file is missing.",
        "pontia workflow show wf_http_submit",
    ] {
        assert!(messages[0].input.contains(text));
    }
    let snapshot = pontia_workflow::WorkflowQueryService::new(app.db.clone(), control.queries())
        .get_workflow("wf_http_submit")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.state, "running");
    assert_eq!(
        snapshot.termination_reason.as_deref(),
        Some("The required source file is missing.")
    );
    assert!(snapshot.nodes[0].submitted_at.is_none());
}
