use std::fs;

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

async fn seed_requester(app: &TestApp) {
    let workflows = SqliteWorkflowRepository::new(app.db.clone());
    workflows
        .create_workflow(CreateWorkflowRecord {
            workflow_id: "wf_patch_request".into(),
            title: "Patch request".into(),
            cwd: app.workspace().path().display().to_string(),
            state: "running".into(),
        })
        .await
        .expect("create workflow");
    workflows
        .create_node(CreateWorkflowNodeRecord {
            node_id: "node_patch_request".into(),
            workflow_id: "wf_patch_request".into(),
            parent_node_id: None,
            phase: "Build".into(),
            title: "Requester".into(),
            instructions: "Request a correction".into(),
            inputs: "[]".into(),
            output: "result.md".into(),
            execution_profile_id: None,
            execution_profile_version: None,
        })
        .await
        .expect("create node");
    sqlx::query(
        "INSERT INTO sessions (session_id, client_type, state) VALUES ('sess_patch_request', 'pi', 'working')",
    )
    .execute(&app.db)
    .await
    .expect("create session");
    workflows
        .bind_node_session("node_patch_request", "sess_patch_request")
        .await
        .expect("bind session");
    sqlx::query(
        r#"INSERT INTO turns (turn_id, session_id, state, topology_status)
           VALUES ('turn_patch_request', 'sess_patch_request', 'running', 'root')"#,
    )
    .execute(&app.db)
    .await
    .expect("create active Turn");
    SqliteRuntimeBindingRepository::new(app.db.clone())
        .upsert_binding(RuntimeBindingUpsertRecord {
            session_id: "sess_patch_request".into(),
            runtime_kind: "pi_tui".into(),
            runtime_instance_id: Some("runtime_patch_request".into()),
            binding_state: "confirmed".into(),
            runtime_handle: None,
            start_command: None,
            launch_cwd: None,
            internal_event_url: None,
            started_at: None,
            last_seen_at: None,
            restart_count: 0,
            tmux_socket_path: Some("/tmp/pontia-test.sock".into()),
            tmux_pane_id: Some("%1".into()),
            process_fingerprint: None,
            capabilities: r#"{"interrupt":true}"#.into(),
            diagnostics: "{}".into(),
            adapter_details: "{}".into(),
        })
        .await
        .expect("create Runtime binding");
    let workflow_dir = app.pontia_home().path().join("workflows/wf_patch_request");
    fs::create_dir_all(&workflow_dir).expect("create Workflow directory");
    fs::write(
        workflow_dir.join("workflow.toml"),
        "workflow_id = \"wf_patch_request\"\n",
    )
    .expect("write accepted definition surface");
}

async fn request_patch(app: &TestApp, runtime: &str) -> (StatusCode, Value) {
    let response = http::router(app.state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/v1/workflow/patches/request")
                .header(header::AUTHORIZATION, "Bearer test-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "session_id": "sess_patch_request",
                        "runtime_instance_id": runtime,
                        "document": "The remaining plan must change. 完成\n"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).expect("JSON response"),
    )
}

async fn block_patch(app: &TestApp, runtime: &str, reason: &str) -> (StatusCode, Value) {
    let response = http::router(app.state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/v1/workflow/patches/block")
                .header(header::AUTHORIZATION, "Bearer test-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "session_id": "sess_patch_replanner",
                        "runtime_instance_id": runtime,
                        "reason": reason,
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn request_is_durable_before_the_coordinator_interrupts() {
    let app = TestApp::new().await;
    seed_requester(&app).await;

    let (status, body) = request_patch(&app, "runtime_patch_request").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let patch_id = body["data"]["patch_id"].as_str().expect("Patch ID");
    assert!(patch_id.starts_with("patch_"));
    assert_eq!(body["data"]["state"], "requested");
    let workflow = SqliteWorkflowRepository::new(app.db.clone())
        .get_workflow("wf_patch_request")
        .await
        .expect("load Workflow")
        .expect("Workflow");
    assert_eq!(workflow.state, "replanning");
    let patch = SqliteWorkflowRepository::new(app.db.clone())
        .get_patch(patch_id)
        .await
        .expect("load Patch")
        .expect("Patch");
    assert_eq!(patch.requesting_turn_id, "turn_patch_request");
    assert_eq!(
        patch.requesting_runtime_instance_id,
        "runtime_patch_request"
    );
    assert!(patch.interruption_attempted_at.is_none());
    assert_eq!(
        fs::read_to_string(
            app.pontia_home()
                .path()
                .join("workflows/wf_patch_request")
                .join(&patch.request_document_ref)
        )
        .expect("request document"),
        "The remaining plan must change. 完成\n"
    );
    let events = SqliteWorkflowRepository::new(app.db.clone())
        .list_events("wf_patch_request")
        .await
        .expect("events");
    assert_eq!(
        events.last().expect("Patch event").event_type,
        "workflow.patch_requested"
    );
    assert!(
        !events
            .last()
            .expect("Patch event")
            .payload
            .contains("remaining plan")
    );
}

#[tokio::test]
async fn active_replanner_can_block_without_supplying_target_identifiers() {
    let app = TestApp::new().await;
    seed_requester(&app).await;
    let (_, requested) = request_patch(&app, "runtime_patch_request").await;
    let patch_id = requested["data"]["patch_id"].as_str().unwrap();
    sqlx::query("INSERT INTO sessions (session_id, client_type, state, current_turn_id) VALUES ('sess_patch_replanner', 'pi', 'busy', 'turn_patch_replanner')")
        .execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO turns (turn_id, session_id, state, topology_status) VALUES ('turn_patch_replanner', 'sess_patch_replanner', 'running', 'root')")
        .execute(&app.db).await.unwrap();
    SqliteRuntimeBindingRepository::new(app.db.clone())
        .upsert_binding(RuntimeBindingUpsertRecord {
            session_id: "sess_patch_replanner".into(),
            runtime_kind: "pi_tui".into(),
            runtime_instance_id: Some("runtime_patch_replanner".into()),
            binding_state: "confirmed".into(),
            runtime_handle: None,
            start_command: None,
            launch_cwd: None,
            internal_event_url: None,
            started_at: None,
            last_seen_at: None,
            restart_count: 0,
            tmux_socket_path: None,
            tmux_pane_id: None,
            process_fingerprint: None,
            capabilities: "{}".into(),
            diagnostics: "{}".into(),
            adapter_details: "{}".into(),
        })
        .await
        .unwrap();
    sqlx::query("UPDATE workflow_patches SET state = 'planning', replanner_session_id = 'sess_patch_replanner', replanner_runtime_instance_id = 'runtime_patch_replanner' WHERE patch_id = ?")
        .bind(patch_id).execute(&app.db).await.unwrap();
    sqlx::query("UPDATE workflows SET active_replanner_session_id = 'sess_patch_replanner' WHERE workflow_id = 'wf_patch_request'")
        .execute(&app.db).await.unwrap();
    let patch_dir = app
        .pontia_home()
        .path()
        .join("workflows/wf_patch_request/patches")
        .join(patch_id);
    fs::copy(
        app.pontia_home()
            .path()
            .join("workflows/wf_patch_request/workflow.toml"),
        patch_dir.join("accepted-definition.toml"),
    )
    .unwrap();

    let (stale_status, _) = block_patch(&app, "stale_runtime", "stale").await;
    assert_eq!(stale_status, StatusCode::CONFLICT);
    assert!(!patch_dir.join("reason.md").exists());
    let (status, body) = block_patch(&app, "runtime_patch_replanner", "Cannot continue").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["patch_id"], patch_id);
    assert_eq!(body["data"]["state"], "blocked");
    assert_eq!(
        fs::read_to_string(patch_dir.join("reason.md")).unwrap(),
        "Cannot continue"
    );
    assert_eq!(
        SqliteWorkflowRepository::new(app.db.clone())
            .get_workflow("wf_patch_request")
            .await
            .unwrap()
            .unwrap()
            .state,
        "blocked"
    );
    assert_eq!(
        block_patch(&app, "runtime_patch_replanner", "again")
            .await
            .0,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn stale_runtime_and_concurrent_request_leave_no_partial_patch() {
    let app = TestApp::new().await;
    seed_requester(&app).await;

    let (status, _) = request_patch(&app, "stale_runtime").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        SqliteWorkflowRepository::new(app.db.clone())
            .get_workflow("wf_patch_request")
            .await
            .expect("load Workflow")
            .expect("Workflow")
            .state,
        "running"
    );

    assert_eq!(
        request_patch(&app, "runtime_patch_request").await.0,
        StatusCode::OK
    );
    assert_eq!(
        request_patch(&app, "runtime_patch_request").await.0,
        StatusCode::CONFLICT
    );
    let patch_dirs = fs::read_dir(
        app.pontia_home()
            .path()
            .join("workflows/wf_patch_request/patches"),
    )
    .expect("Patch directory")
    .count();
    assert_eq!(patch_dirs, 1);
}
