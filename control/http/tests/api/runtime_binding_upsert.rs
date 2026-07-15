use crate::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_application::AppState;
use pontia_http as http;
use serde_json::{Value, json};
use sqlx::Row;
use std::process::{Command, Stdio};
use tower::ServiceExt;

async fn test_state() -> AppState {
    TestApp::builder()
        .database_name("runtime-binding-upsert.db")
        .external_api_token(Some("test-token".to_string()))
        .build_state()
        .await
}

async fn post_upsert(state: AppState, body: Value) -> (StatusCode, Value) {
    request_json(
        state,
        "POST",
        "/internal/v1/runtime-bindings/upsert",
        Some(body),
    )
    .await
}

async fn get_agent_binding_by_client_session(
    state: AppState,
    client_type: &str,
    client_session_key: &str,
) -> (StatusCode, Value) {
    request_json(
        state,
        "GET",
        &format!(
            "/internal/v1/agent-bindings?client_type={client_type}&client_session_key={client_session_key}",
        ),
        None,
    )
    .await
}

async fn get_current_turn_by_client_session(
    state: AppState,
    client_type: &str,
    client_session_key: &str,
) -> (StatusCode, Value) {
    request_json(
        state,
        "GET",
        &format!(
            "/internal/v1/agent-bindings/current-turn?client_type={client_type}&client_session_key={client_session_key}",
        ),
        None,
    )
    .await
}

async fn delete_session(state: AppState, session_id: &str) -> (StatusCode, Value) {
    request_json(
        state,
        "DELETE",
        &format!("/external/v1/sessions/{session_id}"),
        None,
    )
    .await
}

async fn request_json(
    state: AppState,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    if uri.starts_with("/external/v1/") {
        builder = builder.header(header::AUTHORIZATION, "Bearer test-token");
    }
    let response = http::router(state)
        .oneshot(
            builder
                .body(Body::from(
                    body.map(|body| body.to_string()).unwrap_or_default(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

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

#[tokio::test]
async fn internal_agent_bindings_lookup_returns_existing_binding_by_client_session() {
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace.path().to_string_lossy().to_string();

    let (upsert_status, upsert_response) =
        post_upsert(state.clone(), upsert_body(&workspace, Some("%42"))).await;
    assert_eq!(upsert_status, StatusCode::OK, "{upsert_response:?}");
    let session_id = upsert_response["session"]["session_id"]
        .as_str()
        .expect("session id");

    let (lookup_status, lookup_response) =
        get_agent_binding_by_client_session(state, "pi", "pi_session_123").await;

    assert_eq!(lookup_status, StatusCode::OK, "{lookup_response:?}");
    assert_eq!(lookup_response["data"]["binding"]["session_id"], session_id);
    assert_eq!(lookup_response["data"]["binding"]["client_type"], "pi");
    assert_eq!(
        lookup_response["data"]["binding"]["client_session_key"],
        "pi_session_123"
    );
    assert_eq!(lookup_response["data"]["binding"]["launch_cwd"], workspace);
}

#[tokio::test]
async fn internal_agent_bindings_lookup_returns_not_found_for_unknown_client_session() {
    let state = test_state().await;

    let (status, response) =
        get_agent_binding_by_client_session(state, "pi", "missing_session").await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{response:?}");
    assert_eq!(response["error"]["code"], "not_found");
}

#[tokio::test]
async fn internal_agent_binding_current_turn_returns_active_turn_context_by_client_session() {
    let state = test_state().await;
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state, current_turn_id, metadata)
           VALUES ('sess_current', 'claude', 'busy', 'turn_current', '{}')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert session");
    sqlx::query(
        r#"INSERT INTO turns (turn_id, session_id, turn_index, state, input_summary, metadata)
           VALUES ('turn_current', 'sess_current', 1, 'running', 'work', '{}')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert turn");
    sqlx::query(
        r#"INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, launch_cwd, metadata)
           VALUES ('sess_current', 'claude_tui', 'rtinst_current', '/repo', '{"internal_event_url":"http://127.0.0.1:18080/internal/v1/events","extra":"metadata"}')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert runtime binding");
    sqlx::query(
        r#"INSERT INTO agent_bindings (id, session_id, client_type, launch_cwd, client_session_key, metadata)
           VALUES ('binding_current', 'sess_current', 'claude', '/repo', 'claude_session_123', '{"transcript_path":"/tmp/claude.jsonl"}')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert agent binding");

    let (status, response) =
        get_current_turn_by_client_session(state, "claude", "claude_session_123").await;

    assert_eq!(status, StatusCode::OK, "{response:?}");
    let current_turn = &response["data"]["current_turn"];
    assert_eq!(current_turn["session_id"], "sess_current");
    assert_eq!(current_turn["turn_id"], "turn_current");
    assert_eq!(current_turn["client_type"], "claude");
    assert_eq!(current_turn["client_session_key"], "claude_session_123");
    assert_eq!(current_turn["runtime_instance_id"], "rtinst_current");
    assert_eq!(
        current_turn["internal_event_url"],
        "http://127.0.0.1:18080/internal/v1/events"
    );
    assert_eq!(current_turn["runtime_metadata"]["extra"], "metadata");
    assert_eq!(
        current_turn["binding_metadata"]["transcript_path"],
        "/tmp/claude.jsonl"
    );
}

#[tokio::test]
async fn internal_agent_binding_current_turn_returns_not_found_without_active_turn() {
    let state = test_state().await;
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state, current_turn_id, metadata)
           VALUES ('sess_idle', 'claude', 'idle', NULL, '{}')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert session");
    sqlx::query(
        r#"INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, metadata)
           VALUES ('sess_idle', 'claude_tui', 'rtinst_idle', '{}')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert runtime binding");
    sqlx::query(
        r#"INSERT INTO agent_bindings (id, session_id, client_type, launch_cwd, client_session_key, metadata)
           VALUES ('binding_idle', 'sess_idle', 'claude', '/repo', 'claude_idle', '{}')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert agent binding");

    let (status, response) =
        get_current_turn_by_client_session(state, "claude", "claude_idle").await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{response:?}");
    assert_eq!(response["error"]["code"], "not_found");
}

#[tokio::test]
async fn internal_agent_binding_current_turn_returns_not_found_for_unknown_binding() {
    let state = test_state().await;

    let (status, response) = get_current_turn_by_client_session(state, "claude", "missing").await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{response:?}");
    assert_eq!(response["error"]["code"], "not_found");
}

fn upsert_body(workspace: &str, pane_id: Option<&str>) -> Value {
    upsert_body_with_tmux(workspace, "/tmp/tmux-1000/default", pane_id, Some("dev"))
}

fn upsert_body_with_tmux(
    workspace: &str,
    socket_path: &str,
    pane_id: Option<&str>,
    session_name: Option<&str>,
) -> Value {
    let tmux = pane_id.map(|pane_id| {
        json!({
            "socket_path": socket_path,
            "session_id": "$1",
            "session_name": session_name,
            "window_id": "@3",
            "window_index": 0,
            "pane_id": pane_id,
            "pane_index": 1,
            "pane_current_path": workspace
        })
    });
    json!({
        "client_type": "pi",
        "client_session_key": "pi_session_123",
        "client_session_file": "/tmp/pi/session.jsonl",
        "client_session_dir": "/tmp/pi",
        "client_cwd": workspace,
        "launch_cwd": workspace,
        "runtime_instance_id": "rtinst_first",
        "start_command": "pi --approve -e /repo/clients/pi",
        "tmux": tmux
    })
}

#[tokio::test]
async fn fork_upsert_creates_independent_child_session_with_lineage() {
    let pontia_home = tempfile::tempdir().expect("pontia home");
    unsafe {
        std::env::set_var("PONTIA_HOME", pontia_home.path());
    }
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();

    let (parent_status, parent_body) =
        post_upsert(state.clone(), upsert_body(&workspace, Some("%41"))).await;
    assert_eq!(parent_status, StatusCode::OK, "{parent_body:?}");
    let parent_session_id = parent_body["session"]["session_id"]
        .as_str()
        .expect("parent session_id");

    let mut fork_body = upsert_body(&workspace, Some("%42"));
    fork_body["client_session_key"] = json!("pi_session_fork");
    fork_body["runtime_instance_id"] = json!("rtinst_fork");
    fork_body["start_kind"] = json!("fork");
    fork_body["parent_session_id"] = json!(parent_session_id);
    fork_body["forked_from_turn_id"] = json!("turn_parent_1");

    let (fork_status, fork_response) = post_upsert(state.clone(), fork_body).await;

    assert_eq!(fork_status, StatusCode::OK, "{fork_response:?}");
    let child_session_id = fork_response["session"]["session_id"]
        .as_str()
        .expect("child session_id");
    assert_ne!(child_session_id, parent_session_id);
    assert_eq!(fork_response["session"]["lineage"]["relation_type"], "fork");
    assert_eq!(
        fork_response["session"]["lineage"]["parent_session_id"],
        parent_session_id
    );
    assert_eq!(
        fork_response["session"]["lineage"]["forked_from_turn_id"],
        "turn_parent_1"
    );

    let row = sqlx::query(
        "SELECT relation_type, parent_session_id, forked_from_turn_id, parent_client_session_key, child_client_session_key FROM session_lineage WHERE child_session_id = ?",
    )
    .bind(child_session_id)
    .fetch_one(&state.db())
    .await
    .expect("lineage row");
    assert_eq!(row.get::<String, _>("relation_type"), "fork");
    assert_eq!(row.get::<String, _>("parent_session_id"), parent_session_id);
    assert_eq!(
        row.get::<Option<String>, _>("forked_from_turn_id")
            .as_deref(),
        Some("turn_parent_1")
    );
    assert_eq!(
        row.get::<Option<String>, _>("parent_client_session_key")
            .as_deref(),
        Some("pi_session_123")
    );
    assert_eq!(
        row.get::<Option<String>, _>("child_client_session_key")
            .as_deref(),
        Some("pi_session_fork")
    );

    let (get_status, get_body) = request_json(
        state,
        "GET",
        &format!("/external/v1/sessions/{child_session_id}"),
        None,
    )
    .await;
    assert_eq!(get_status, StatusCode::OK, "{get_body:?}");
    assert_eq!(
        get_body["data"]["session"]["lineage"]["relation_type"],
        "fork"
    );
    assert_eq!(
        get_body["data"]["session"]["lineage"]["parent_session_id"],
        parent_session_id
    );
}

#[tokio::test]
async fn upsert_creates_session_runtime_binding_and_agent_binding_for_tmux_pi() {
    let pontia_home = tempfile::tempdir().expect("pontia home");
    unsafe {
        std::env::set_var("PONTIA_HOME", pontia_home.path());
    }
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();

    let (status, body) = post_upsert(state.clone(), upsert_body(&workspace, Some("%42"))).await;

    assert_eq!(status, StatusCode::OK, "{body:?}");
    let session_id = body["session"]["session_id"].as_str().expect("session_id");
    assert!(session_id.starts_with("sess_"));
    assert_eq!(body["runtime"]["runtime_instance_id"], "rtinst_first");
    assert!(
        body["runtime"]["internal_event_url"]
            .as_str()
            .unwrap()
            .ends_with("/internal/v1/events")
    );
    assert_eq!(body["runtime"]["capabilities"]["accept_task"], true);
    assert_eq!(body["runtime"]["capabilities"]["interrupt"], true);
    assert_eq!(body["runtime"]["capabilities"]["stream_output"], true);
    assert_eq!(
        body["runtime"]["capabilities"]["context_usage"],
        "estimated"
    );
    assert_eq!(body["runtime"]["capabilities"]["report_turn_started"], true);
    assert_eq!(
        body["runtime"]["capabilities"]["report_turn_finished"],
        true
    );

    let row = sqlx::query(
        "SELECT runtime_kind, runtime_instance_id, start_command, launch_cwd, tmux_socket_path, tmux_pane_id, metadata FROM runtime_bindings WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_one(&state.db())
    .await
    .expect("runtime binding");
    assert_eq!(row.get::<String, _>("runtime_kind"), "pi_tui");
    assert_eq!(row.get::<String, _>("runtime_instance_id"), "rtinst_first");
    assert_eq!(
        row.get::<String, _>("start_command"),
        "pi --approve -e /repo/clients/pi"
    );
    assert_eq!(row.get::<String, _>("launch_cwd"), workspace);
    assert_eq!(
        row.get::<String, _>("tmux_socket_path"),
        "/tmp/tmux-1000/default"
    );
    assert_eq!(row.get::<String, _>("tmux_pane_id"), "%42");
    let metadata: Value = serde_json::from_str(&row.get::<String, _>("metadata")).unwrap();
    assert_eq!(metadata["client_session_key"], "pi_session_123");
    assert_eq!(metadata["tmux"]["session_name"], "dev");
    assert_eq!(metadata["capabilities"]["accept_task"], true);
    assert_eq!(metadata["capabilities"]["context_usage"], "estimated");
    let expected_state_dir = pontia_home.path().join("state");
    assert_eq!(
        metadata["log_dir"],
        expected_state_dir.display().to_string()
    );
    assert_eq!(
        metadata["runtime_log"],
        expected_state_dir.join("runtime.log").display().to_string()
    );
    assert_eq!(
        metadata["pi_hook_log"],
        expected_state_dir.join("pi-hook.log").display().to_string()
    );
    unsafe {
        std::env::remove_var("PONTIA_HOME");
    }

    let binding_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_bindings WHERE session_id = ? AND client_type = 'pi' AND client_session_key = 'pi_session_123'")
        .bind(session_id)
        .fetch_one(&state.db())
        .await
        .expect("agent binding count");
    assert_eq!(binding_count, 1);
}

#[tokio::test]
async fn upsert_marks_bound_tmux_pane_as_pontia_owned() {
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();
    let tmux_session = format!("pontia_manual_mark_{}", std::process::id());
    let _guard = TmuxSessionGuard(tmux_session.clone());
    let status = Command::new("tmux")
        .args(["new-session", "-d", "-s", &tmux_session, "sh"])
        .stderr(Stdio::null())
        .status()
        .expect("spawn tmux");
    assert!(status.success(), "tmux session should start");
    let socket_path = tmux_display(&tmux_session, "#{socket_path}");
    let pane_id = tmux_display(&tmux_session, "#{pane_id}");

    let (status, body) = post_upsert(
        state.clone(),
        upsert_body_with_tmux(
            &workspace,
            &socket_path,
            Some(&pane_id),
            Some(&tmux_session),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body:?}");
    let session_id = body["session"]["session_id"].as_str().expect("session_id");
    assert_eq!(tmux_display(&pane_id, "#{@pontia_session_id}"), session_id);
    assert_eq!(
        tmux_display(&pane_id, "#{@pontia_runtime_instance_id}"),
        "rtinst_first"
    );
}

#[tokio::test]
async fn upsert_is_idempotent_for_same_pi_session_key_and_refreshes_runtime_fields() {
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();

    let (first_status, first) =
        post_upsert(state.clone(), upsert_body(&workspace, Some("%42"))).await;
    assert_eq!(first_status, StatusCode::OK, "{first:?}");
    let first_session_id = first["session"]["session_id"].as_str().unwrap().to_string();

    let mut second_body = upsert_body(&workspace, Some("%99"));
    second_body["runtime_instance_id"] = json!("rtinst_second");
    second_body["start_command"] = json!("pi --resume");
    let (second_status, second) = post_upsert(state.clone(), second_body).await;

    assert_eq!(second_status, StatusCode::OK, "{second:?}");
    assert_eq!(second["session"]["session_id"], first_session_id);
    assert_eq!(second["runtime"]["runtime_instance_id"], "rtinst_second");

    let session_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&state.db())
        .await
        .expect("session count");
    assert_eq!(session_count, 1);

    let row = sqlx::query("SELECT runtime_instance_id, start_command, tmux_pane_id FROM runtime_bindings WHERE session_id = ?")
        .bind(&first_session_id)
        .fetch_one(&state.db())
        .await
        .expect("runtime binding");
    assert_eq!(row.get::<String, _>("runtime_instance_id"), "rtinst_second");
    assert_eq!(row.get::<String, _>("start_command"), "pi --resume");
    assert_eq!(row.get::<String, _>("tmux_pane_id"), "%99");

    let binding_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_bindings")
        .fetch_one(&state.db())
        .await
        .expect("agent binding count");
    assert_eq!(binding_count, 1);
}

#[tokio::test]
async fn upsert_creates_a_new_session_for_a_new_pi_session_key() {
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();

    let (first_status, first) =
        post_upsert(state.clone(), upsert_body(&workspace, Some("%42"))).await;
    assert_eq!(first_status, StatusCode::OK, "{first:?}");

    let mut second_request = upsert_body(&workspace, Some("%43"));
    second_request["client_session_key"] = json!("pi_session_456");
    second_request["runtime_instance_id"] = json!("rtinst_second");
    let (second_status, second) = post_upsert(state.clone(), second_request).await;

    assert_eq!(second_status, StatusCode::OK, "{second:?}");
    assert_ne!(
        second["session"]["session_id"],
        first["session"]["session_id"]
    );
    let session_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&state.db())
        .await
        .expect("session count");
    let binding_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_bindings")
        .fetch_one(&state.db())
        .await
        .expect("binding count");
    assert_eq!(session_count, 2);
    assert_eq!(binding_count, 2);
}

#[tokio::test]
async fn concurrent_first_upserts_for_one_pi_session_key_reuse_one_session() {
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();
    let first_request = upsert_body(&workspace, None);
    let mut second_request = upsert_body(&workspace, None);
    second_request["runtime_instance_id"] = json!("rtinst_concurrent_second");

    let (first, second) = tokio::join!(
        post_upsert(state.clone(), first_request),
        post_upsert(state.clone(), second_request)
    );

    assert_eq!(first.0, StatusCode::OK, "{:?}", first.1);
    assert_eq!(second.0, StatusCode::OK, "{:?}", second.1);
    assert_eq!(
        first.1["session"]["session_id"],
        second.1["session"]["session_id"]
    );
    let session_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&state.db())
        .await
        .expect("session count");
    let binding_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_bindings")
        .fetch_one(&state.db())
        .await
        .expect("binding count");
    assert_eq!(session_count, 1);
    assert_eq!(binding_count, 1);
}

#[tokio::test]
async fn upsert_rejects_a_runtime_binding_that_disagrees_with_the_agent_binding() {
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();

    let (first_status, first) =
        post_upsert(state.clone(), upsert_body(&workspace, Some("%42"))).await;
    assert_eq!(first_status, StatusCode::OK, "{first:?}");
    let session_id = first["session"]["session_id"].as_str().expect("session id");
    sqlx::query(
        "UPDATE runtime_bindings SET metadata = json_set(metadata, '$.client_session_key', 'pi_conflicting') WHERE session_id = ?",
    )
    .bind(session_id)
    .execute(&state.db())
    .await
    .expect("corrupt runtime binding identity");

    let mut retry = upsert_body(&workspace, Some("%99"));
    retry["runtime_instance_id"] = json!("rtinst_rejected");
    let (status, body) = post_upsert(state.clone(), retry).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"]["code"], "state_conflict");
    assert!(
        body["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("does not match")
    );
    let runtime_instance_id: String =
        sqlx::query_scalar("SELECT runtime_instance_id FROM runtime_bindings WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&state.db())
            .await
            .expect("runtime binding");
    assert_eq!(runtime_instance_id, "rtinst_first");
}

#[tokio::test]
async fn upsert_existing_exited_pi_session_records_resume_lifecycle() {
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();

    let (first_status, first) =
        post_upsert(state.clone(), upsert_body(&workspace, Some("%42"))).await;
    assert_eq!(first_status, StatusCode::OK, "{first:?}");
    let session_id = first["session"]["session_id"].as_str().unwrap().to_string();

    let (exit_status, exit) = request_json(
        state.clone(),
        "POST",
        "/internal/v1/events",
        Some(json!({
            "event_id": "evt_upsert_existing_exit",
            "session_id": session_id,
            "turn_id": null,
            "source": "agent_client",
            "client_type": "pi",
            "type": "session.exited",
            "time": "2026-01-01T00:00:00Z",
            "seq": null,
            "payload": { "runtime_instance_id": "rtinst_first", "reason": "quit" }
        })),
    )
    .await;
    assert_eq!(exit_status, StatusCode::OK, "{exit:?}");

    let mut second_body = upsert_body(&workspace, Some("%99"));
    second_body["runtime_instance_id"] = json!("rtinst_second");
    let (second_status, second) = post_upsert(state.clone(), second_body).await;
    assert_eq!(second_status, StatusCode::OK, "{second:?}");
    assert_eq!(second["session"]["session_id"], session_id);

    let state_after_upsert: String =
        sqlx::query_scalar("SELECT state FROM sessions WHERE session_id = ?")
            .bind(&session_id)
            .fetch_one(&state.db())
            .await
            .expect("session state");
    assert_eq!(state_after_upsert, "starting");

    let event_types: Vec<String> = sqlx::query_scalar(
        "SELECT event_type FROM events WHERE session_id = ? ORDER BY rowid DESC LIMIT 2",
    )
    .bind(&session_id)
    .fetch_all(&state.db())
    .await
    .expect("event types");
    assert_eq!(event_types, vec!["session.started", "session.resuming"]);
}

#[tokio::test]
async fn upsert_non_tmux_pi_session_is_observable_but_not_web_writable() {
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();

    let (status, body) = post_upsert(state.clone(), upsert_body(&workspace, None)).await;

    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["runtime"]["capabilities"]["accept_task"], false);
    assert_eq!(body["runtime"]["capabilities"]["interrupt"], false);
    assert_eq!(body["runtime"]["capabilities"]["stream_output"], true);
    assert_eq!(
        body["runtime"]["capabilities"]["context_usage"],
        "estimated"
    );
    assert_eq!(body["runtime"]["capabilities"]["report_turn_started"], true);
    assert_eq!(
        body["runtime"]["capabilities"]["report_turn_finished"],
        true
    );

    let session_id = body["session"]["session_id"].as_str().unwrap();
    let row = sqlx::query("SELECT tmux_socket_path, tmux_pane_id, metadata FROM runtime_bindings WHERE session_id = ?")
        .bind(session_id)
        .fetch_one(&state.db())
        .await
        .expect("runtime binding");
    assert!(row.get::<Option<String>, _>("tmux_socket_path").is_none());
    assert!(row.get::<Option<String>, _>("tmux_pane_id").is_none());
    let metadata: Value = serde_json::from_str(&row.get::<String, _>("metadata")).unwrap();
    assert_eq!(metadata["capabilities"]["accept_task"], false);
    assert_eq!(metadata["capabilities"]["context_usage"], "estimated");
}

#[tokio::test]
async fn webui_resume_of_manually_bound_tui_appends_pi_session_id_to_start_command() {
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();

    let mut body = upsert_body(&workspace, Some("%42"));
    body["start_command"] = json!("pi");
    let (upsert_status, upsert) = post_upsert(state.clone(), body).await;
    assert_eq!(upsert_status, StatusCode::OK, "{upsert:?}");
    let session_id = upsert["session"]["session_id"].as_str().unwrap();

    let (exit_status, exit) = request_json(
        state.clone(),
        "POST",
        "/internal/v1/events",
        Some(json!({
            "event_id": "evt_manual_tui_exit_before_webui_resume",
            "session_id": session_id,
            "turn_id": null,
            "source": "agent_client",
            "client_type": "pi",
            "type": "session.exited",
            "time": "2026-01-01T00:00:00Z",
            "seq": null,
            "payload": { "runtime_instance_id": "rtinst_first", "reason": "quit" }
        })),
    )
    .await;
    assert_eq!(exit_status, StatusCode::OK, "{exit:?}");

    let (resume_status, resume) = request_json(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/resume"),
        None,
    )
    .await;
    assert_eq!(resume_status, StatusCode::OK, "{resume:?}");

    let start_command: String =
        sqlx::query_scalar("SELECT start_command FROM runtime_bindings WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&state.db())
            .await
            .expect("runtime start command");
    assert_eq!(start_command, "pi --session-id 'pi_session_123'");
}

#[tokio::test]
async fn terminate_manually_bound_tui_without_pane_binding_marks_session_exited() {
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();

    let (upsert_status, upsert) = post_upsert(
        state.clone(),
        upsert_body_with_tmux(
            &workspace,
            "/tmp/tmux-1000/default",
            Some("%42"),
            Some("old-dev"),
        ),
    )
    .await;
    assert_eq!(upsert_status, StatusCode::OK, "{upsert:?}");
    let session_id = upsert["session"]["session_id"].as_str().unwrap();

    sqlx::query(
        "UPDATE runtime_bindings SET tmux_socket_path = NULL, tmux_pane_id = NULL WHERE session_id = ?",
    )
    .bind(session_id)
    .execute(&state.db())
    .await
    .expect("remove pane binding");

    let (terminate_status, terminate) = delete_session(state.clone(), session_id).await;

    assert_eq!(terminate_status, StatusCode::OK, "{terminate:?}");
    assert_eq!(terminate["data"]["session"]["state"], "exited");

    let exit_event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE session_id = ? AND event_type = 'session.exited'",
    )
    .bind(session_id)
    .fetch_one(&state.db())
    .await
    .expect("exit event count");
    assert_eq!(exit_event_count, 1);
}

#[tokio::test]
async fn terminate_non_tmux_manually_bound_tui_marks_session_exited() {
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();

    let (upsert_status, upsert) = post_upsert(
        state.clone(),
        upsert_body_with_tmux(&workspace, "/tmp/tmux-1000/default", None, None),
    )
    .await;
    assert_eq!(upsert_status, StatusCode::OK, "{upsert:?}");
    let session_id = upsert["session"]["session_id"].as_str().unwrap();

    let (terminate_status, terminate) = delete_session(state.clone(), session_id).await;

    assert_eq!(terminate_status, StatusCode::OK, "{terminate:?}");
    assert_eq!(terminate["data"]["session"]["state"], "exited");
}

#[tokio::test]
async fn terminate_manually_bound_tui_session_sends_pi_exit_sequence_to_bound_pane() {
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();
    let signal_log = tempfile::NamedTempFile::new().expect("signal log");
    let signal_log_path = signal_log.path().display().to_string();
    let tmux_session = format!("pontia_manual_terminate_{}", std::process::id());
    let _guard = TmuxSessionGuard(tmux_session.clone());

    let command = format!(
        "python3 -c {}",
        shell_quote(&format!(
            "import signal,time; f=open({:?}, 'a', buffering=1); signal.signal(signal.SIGINT, lambda *_: f.write('int\\n')); time.sleep(30)",
            signal_log_path
        ))
    );
    let status = Command::new("tmux")
        .args(["new-session", "-d", "-s", &tmux_session, &command])
        .status()
        .expect("spawn tmux");
    assert!(status.success(), "tmux session should start");
    std::thread::sleep(std::time::Duration::from_millis(500));
    let socket_path = tmux_display(&tmux_session, "#{socket_path}");
    let pane_id = tmux_display(&tmux_session, "#{pane_id}");

    let (upsert_status, upsert) = post_upsert(
        state.clone(),
        upsert_body_with_tmux(&workspace, &socket_path, Some(&pane_id), None),
    )
    .await;
    assert_eq!(upsert_status, StatusCode::OK, "{upsert:?}");
    let session_id = upsert["session"]["session_id"].as_str().unwrap();

    let (terminate_status, terminate) = delete_session(state.clone(), session_id).await;

    assert_eq!(terminate_status, StatusCode::OK, "{terminate:?}");
    assert_eq!(terminate["data"]["session"]["state"], "exited");
    assert!(tmux_pane_alive(&socket_path, &pane_id));
    assert_eq!(wait_for_signal_count(signal_log.path(), 2), 2);
}

fn tmux_display(target: &str, format: &str) -> String {
    let output = Command::new("tmux")
        .args(["display-message", "-p", "-t", target, format])
        .output()
        .expect("tmux display");
    assert!(output.status.success(), "tmux display should succeed");
    String::from_utf8(output.stdout)
        .expect("utf8")
        .trim()
        .to_string()
}

fn tmux_pane_alive(socket_path: &str, pane_id: &str) -> bool {
    let output = Command::new("tmux")
        .args(["-S", socket_path, "list-panes", "-a", "-F", "#{pane_id}"])
        .stderr(Stdio::null())
        .output();
    output.is_ok_and(|output| {
        output.status.success()
            && String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| line == pane_id)
    })
}

fn wait_for_signal_count(path: &std::path::Path, expected: usize) -> usize {
    for _ in 0..20 {
        let count = std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .count();
        if count >= expected {
            return count;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .count()
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

struct TmuxSessionGuard(String);

impl Drop for TmuxSessionGuard {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &self.0])
            .stderr(Stdio::null())
            .status();
    }
}
