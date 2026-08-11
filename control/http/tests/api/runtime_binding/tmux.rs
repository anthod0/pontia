use super::{
    StatusCode, delete_session, json, post_upsert, request_json, test_state, upsert_body_with_tmux,
};
use std::process::{Command, Stdio};
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
        body["runtime"]["runtime_instance_id"].as_str().unwrap()
    );
}

#[tokio::test]
async fn session_exit_clears_matching_pontia_markers_from_the_bound_tmux_pane() {
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();
    let tmux_session = format!("pontia_exit_unmark_{}", std::process::id());
    let _guard = TmuxSessionGuard(tmux_session.clone());
    let status = Command::new("tmux")
        .args(["new-session", "-d", "-s", &tmux_session, "sh"])
        .stderr(Stdio::null())
        .status()
        .expect("spawn tmux");
    assert!(status.success(), "tmux session should start");
    let socket_path = tmux_display(&tmux_session, "#{socket_path}");
    let pane_id = tmux_display(&tmux_session, "#{pane_id}");

    let (upsert_status, upsert) = post_upsert(
        state.clone(),
        upsert_body_with_tmux(
            &workspace,
            &socket_path,
            Some(&pane_id),
            Some(&tmux_session),
        ),
    )
    .await;
    assert_eq!(upsert_status, StatusCode::OK, "{upsert:?}");
    let session_id = upsert["session"]["session_id"].as_str().unwrap();
    let runtime_instance_id = upsert["runtime"]["runtime_instance_id"].as_str().unwrap();

    let (exit_status, exit) = request_json(
        state,
        "POST",
        "/internal/v1/events",
        Some(json!({
            "session_id": session_id,
            "type": "session.exited",
            "data": { "runtime_instance_id": runtime_instance_id, "reason": "quit" }
        })),
    )
    .await;

    assert_eq!(exit_status, StatusCode::OK, "{exit:?}");
    assert_eq!(tmux_display(&pane_id, "#{@pontia_session_id}"), "");
    assert_eq!(tmux_display(&pane_id, "#{@pontia_runtime_instance_id}"), "");
}

#[tokio::test]
async fn terminate_manually_bound_tui_without_pane_binding_is_rejected() {
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

    assert_eq!(
        terminate_status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{terminate:?}"
    );
    assert_eq!(terminate["error"]["code"], "capability_unavailable");

    let exit_event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE session_id = ? AND event_type = 'session.exited'",
    )
    .bind(session_id)
    .fetch_one(&state.db())
    .await
    .expect("exit event count");
    assert_eq!(exit_event_count, 0);
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
    assert_ne!(terminate["data"]["session"]["state"], "exited");
    assert!(tmux_pane_alive(&socket_path, &pane_id));
    assert_eq!(tmux_display(&pane_id, "#{@pontia_session_id}"), session_id);
    assert_eq!(
        tmux_display(&pane_id, "#{@pontia_runtime_instance_id}"),
        upsert["runtime"]["runtime_instance_id"].as_str().unwrap()
    );
    assert_eq!(wait_for_signal_count(signal_log.path(), 2), 2);

    let exit_event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE session_id = ? AND event_type = 'session.exited'",
    )
    .bind(session_id)
    .fetch_one(&state.db())
    .await
    .expect("exit event count");
    assert_eq!(exit_event_count, 0);
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
