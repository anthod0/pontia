use std::{fs, os::unix::fs::PermissionsExt};

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

use crate::test_app::TestApp;

async fn seed_running_workflow(app: &TestApp) {
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
            title: "Writer".to_string(),
            instructions: "Write the handoff".to_string(),
            inputs: "[]".to_string(),
            output: "result.md".to_string(),
            execution_profile_id: None,
            execution_profile_version: None,
        })
        .await
        .expect("create node");
    sqlx::query(
        "INSERT INTO sessions (session_id, client_type, state) VALUES (?, 'pi', 'working')",
    )
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
            start_command: None,
            launch_cwd: Some(app.workspace().path().display().to_string()),
            last_seen_at: None,
            tmux_socket_path: Some("/tmp/fake-pontia-tmux.sock".to_string()),
            tmux_pane_id: Some("%42".to_string()),
            metadata: "{}".to_string(),
        })
        .await
        .expect("create runtime binding");
    fs::create_dir_all(
        app.pontia_home()
            .path()
            .join("workflows/wf_http_submit/handoff"),
    )
    .expect("create handoff directory");
}

fn install_successful_tmux(app: &mut TestApp) {
    let bin = app.pontia_home().path().join("test-bin");
    fs::create_dir(&bin).expect("create test bin");
    let tmux = bin.join("tmux");
    fs::write(
        &tmux,
        "#!/bin/sh\ncase \"$*\" in\n  *list-panes*) printf '%%42\\n' ;;\nesac\nexit 0\n",
    )
    .expect("write fake tmux");
    let mut permissions = fs::metadata(&tmux)
        .expect("fake tmux metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(tmux, permissions).expect("make fake tmux executable");
    app.set_env(
        "PATH",
        format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        ),
    );
}

async fn post_submission(app: &TestApp, body: Value) -> (StatusCode, Value) {
    post_submission_with_auth(app, body, Some("Bearer test-token")).await
}

async fn post_submission_with_auth(
    app: &TestApp,
    body: Value,
    authorization: Option<&str>,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri("/internal/v1/workflow/submissions")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(authorization) = authorization {
        request = request.header(header::AUTHORIZATION, authorization);
    }
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
async fn internal_workflow_submission_saves_handoff_and_requests_exit() {
    let mut app = TestApp::new().await;
    seed_running_workflow(&app).await;
    install_successful_tmux(&mut app);

    let (status, body) = post_submission(
        &app,
        json!({
            "session_id": "sess_http_submit",
            "runtime_instance_id": "rtinst_http_submit",
            "output": "result.md",
            "content": "Submitted through HTTP: 完成\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({ "data": { "submitted": true } }));
    assert_eq!(
        fs::read_to_string(
            app.pontia_home()
                .path()
                .join("workflows/wf_http_submit/handoff/result.md")
        )
        .expect("read handoff"),
        "Submitted through HTTP: 完成\n"
    );
    let node = SqliteWorkflowRepository::new(app.db.clone())
        .get_node("node_http_submit")
        .await
        .expect("load node")
        .expect("node exists");
    assert!(node.submitted_at.is_some());
}

#[tokio::test]
async fn internal_workflow_submission_requires_local_api_authentication() {
    let app = TestApp::new().await;
    let request = json!({
        "session_id": "sess_http_submit",
        "runtime_instance_id": "rtinst_http_submit",
        "output": "result.md",
        "content": "must not be accepted"
    });

    for authorization in [None, Some("Bearer wrong-token")] {
        let (status, body) = post_submission_with_auth(&app, request.clone(), authorization).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
        assert_eq!(body["error"]["code"], "authentication_failed");
    }
}

#[tokio::test]
async fn internal_workflow_submission_preserves_service_conflicts() {
    let mut app = TestApp::new().await;
    seed_running_workflow(&app).await;
    install_successful_tmux(&mut app);

    let (status, body) = post_submission(
        &app,
        json!({
            "session_id": "sess_http_submit",
            "runtime_instance_id": "rtinst_http_submit",
            "output": "unexpected.md",
            "content": "must not be written"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "state_conflict");
    assert!(
        body["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("declared output")
    );
    assert!(
        !app.pontia_home()
            .path()
            .join("workflows/wf_http_submit/handoff/result.md")
            .exists()
    );
}
