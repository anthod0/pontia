use std::process::{Command, Stdio};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use llmparty::{
    application::AppState,
    storage::sqlite::{connect_sqlite, run_migrations},
    transport::http,
};
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "test-token";

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("global_workspace_tasks.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    AppState {
        db,
        external_api_token: Some(TOKEN.to_string()),
    }
}

async fn post_json(state: AppState, uri: &str, body: Value) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    json_response(response).await
}

async fn get_json(state: AppState, uri: &str) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    json_response(response).await
}

async fn json_response(response: axum::response::Response) -> (StatusCode, Value) {
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let json = serde_json::from_slice(&body).expect("json body");
    (status, json)
}

struct TmuxSessionGuard {
    tmux_session: String,
}

impl TmuxSessionGuard {
    fn for_session(session_id: &str) -> Self {
        let sanitized: String = session_id
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect();
        Self {
            tmux_session: format!("llmparty_{sanitized}"),
        }
    }
}

impl Drop for TmuxSessionGuard {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &self.tmux_session])
            .stderr(Stdio::null())
            .status();
    }
}

#[tokio::test]
async fn create_session_upserts_canonical_workspace_and_links_session() {
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let canonical = std::fs::canonicalize(workspace.path()).expect("canonical");

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/sessions",
        json!({"client_type":"generic", "workspace": workspace.path().display().to_string()}),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let session_id = body["data"]["session"]["session_id"].as_str().unwrap();
    let _runtime_guard = TmuxSessionGuard::for_session(session_id);
    assert_eq!(
        body["data"]["session"]["workspace"],
        canonical.display().to_string()
    );
    let workspace_id = body["data"]["session"]["workspace_id"]
        .as_str()
        .expect("workspace id");
    assert!(workspace_id.starts_with("wks_"));

    let (status, body) = get_json(state, "/external/v1/workspaces").await;
    assert_eq!(status, StatusCode::OK);
    let workspaces = body["data"]["workspaces"].as_array().unwrap();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0]["workspace_id"], workspace_id);
    assert_eq!(
        workspaces[0]["canonical_path"],
        canonical.display().to_string()
    );
    assert_eq!(workspaces[0]["state"], "active");
}

#[tokio::test]
async fn create_task_without_workspace_persists_global_task_for_confirmation() {
    let state = test_state().await;

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/tasks",
        json!({"input":"find the right workspace", "client_type":"generic"}),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let task = &body["data"]["task"];
    let task_id = task["task_id"].as_str().expect("task id");
    assert!(task_id.starts_with("task_"));
    assert_eq!(task["state"], "needs_confirmation");
    assert_eq!(task["routing_state"], "ambiguous");
    assert_eq!(task["workspace_id"], Value::Null);
    assert_eq!(task["session_id"], Value::Null);
    assert_eq!(task["turn_id"], Value::Null);

    let (status, body) = get_json(state, "/external/v1/tasks").await;
    assert_eq!(status, StatusCode::OK);
    let tasks = body["data"]["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["task_id"], task_id);
}

#[tokio::test]
async fn create_task_with_workspace_routes_to_session_and_links_created_turn() {
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let canonical = std::fs::canonicalize(workspace.path()).expect("canonical");

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/tasks",
        json!({
            "input":"run this globally",
            "workspace": workspace.path().display().to_string(),
            "client_type":"generic",
            "metadata":{"source":"test"}
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let task = &body["data"]["task"];
    let session_id = task["session_id"].as_str().expect("session id");
    let _runtime_guard = TmuxSessionGuard::for_session(session_id);
    let turn_id = task["turn_id"].as_str().expect("turn id");
    assert_eq!(task["state"], "queued");
    assert_eq!(task["routing_state"], "matched");
    assert_eq!(task["input"], "run this globally");
    assert!(task["workspace_id"].as_str().unwrap().starts_with("wks_"));

    let (status, session_body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        session_body["data"]["session"]["workspace"],
        canonical.display().to_string()
    );

    let (status, turn_body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        turn_body["data"]["turn"]["input"]["summary"],
        "run this globally"
    );

    let (status, task_body) = get_json(
        state,
        &format!("/external/v1/tasks/{}", task["task_id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(task_body["data"]["task"]["turn_id"], turn_id);
}
