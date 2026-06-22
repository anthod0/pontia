use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_application::AppState;
use pontia_http as http;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use sqlx::Row;
use std::process::{Command, Stdio};
use tower::ServiceExt;

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("runtime-binding-upsert.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    AppState::builder(db)
        .external_api_token(Some("test-token".to_string()))
        .build()
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
async fn upsert_creates_session_runtime_binding_and_agent_binding_for_tmux_pi() {
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
