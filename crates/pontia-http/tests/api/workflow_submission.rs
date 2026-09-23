use std::{fs, sync::Arc};

use pontia_client_pi::rpc::{PROTOCOL_VERSION, PiRpcPeer, RpcRequest};
use tokio::net::UnixStream;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_http as http;
use pontia_storage_sqlite::repositories::{
    runtime_bindings::{RuntimeBindingUpsertRecord, SqliteRuntimeBindingRepository},
    workflows::{CreateWorkflowNodeRecord, CreateWorkflowRecord, SqliteWorkflowRepository},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::common::test_app::TestApp;

async fn seed_running_workflow(
    app: &TestApp,
) -> (Arc<PiRpcPeer>, tokio::sync::mpsc::Receiver<RpcRequest>) {
    let workflows = SqliteWorkflowRepository::new(app.db.clone());
    workflows
        .create_workflow(CreateWorkflowRecord {
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
            execution_profile_version: None,
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
    SqliteRuntimeBindingRepository::new(app.db.clone())
        .upsert_binding(RuntimeBindingUpsertRecord {
            session_id: "sess_http_submit".to_string(),
            runtime_kind: "pi_tui".to_string(),
            runtime_instance_id: Some("rtinst_http_submit".to_string()),
            binding_state: "confirmed".to_string(),
            runtime_handle: None,
            start_command: None,
            launch_cwd: Some(app.workspace().path().display().to_string()),
            started_at: None,
            last_seen_at: None,
            restart_count: 0,
            tmux_socket_path: Some("/tmp/fake-pontia-tmux.sock".to_string()),
            tmux_pane_id: Some("%42".to_string()),
            process_fingerprint: None,
            capabilities: "{}".to_string(),
            diagnostics: "{}".to_string(),
            adapter_details: "{}".to_string(),
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
        app.state.clone(),
        server,
    ));
    let (client, requests) = PiRpcPeer::new(client);
    client
        .call(
            "runtime.attach",
            json!({
                "version": PROTOCOL_VERSION,
                "session_id": "sess_http_submit",
                "runtime_instance_id": "rtinst_http_submit",
                "client_session_key": "native_http_submit",
            }),
        )
        .await
        .expect("attach Pi control channel");
    (client, requests)
}

async fn post_submission(app: &TestApp, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/workflow/submissions")
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

    let (status, body) = post_submission(
        &app,
        json!({
            "session_id": "sess_http_submit",
            "runtime_instance_id": "rtinst_http_submit"
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
    let workflows = SqliteWorkflowRepository::new(app.db.clone());
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

    let (status, body) = post_submission(
        &app,
        json!({
            "session_id": "sess_http_submit",
            "runtime_instance_id": "rtinst_http_submit"
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
