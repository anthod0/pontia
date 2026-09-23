use super::{StatusCode, delete_session, json, post_upsert, test_state, upsert_body_with_tmux};
use std::process::{Command, Stdio};
#[tokio::test]
async fn upsert_marks_bound_tmux_pane_as_pontia_owned() {
    let (state, _app) = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();
    let tmux_session = format!("pontia_manual_mark_{}", std::process::id());
    let test_socket = _app.pontia_home().path().join("tmux.sock");
    let _guard = TmuxSessionGuard(test_socket.clone());
    let status = Command::new("tmux")
        .arg("-S")
        .arg(&test_socket)
        .args(["new-session", "-d", "-s", &tmux_session, "sh"])
        .stderr(Stdio::null())
        .status()
        .expect("spawn tmux");
    assert!(status.success(), "tmux session should start");
    let socket_path = tmux_display(&test_socket, &tmux_session, "#{socket_path}");
    let pane_id = tmux_display(&test_socket, &tmux_session, "#{pane_id}");

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
    assert_eq!(
        tmux_display(&test_socket, &pane_id, "#{@pontia_session_id}"),
        session_id
    );
    assert_eq!(
        tmux_display(&test_socket, &pane_id, "#{@pontia_runtime_instance_id}"),
        body["runtime"]["runtime_instance_id"].as_str().unwrap()
    );
}

#[tokio::test]
async fn session_exit_clears_matching_pontia_markers_from_the_bound_tmux_pane() {
    let (state, _app) = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();
    let tmux_session = format!("pontia_exit_unmark_{}", std::process::id());
    let test_socket = _app.pontia_home().path().join("tmux.sock");
    let _guard = TmuxSessionGuard(test_socket.clone());
    let status = Command::new("tmux")
        .arg("-S")
        .arg(&test_socket)
        .args(["new-session", "-d", "-s", &tmux_session, "sh"])
        .stderr(Stdio::null())
        .status()
        .expect("spawn tmux");
    assert!(status.success(), "tmux session should start");
    let socket_path = tmux_display(&test_socket, &tmux_session, "#{socket_path}");
    let pane_id = tmux_display(&test_socket, &tmux_session, "#{pane_id}");

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

    let (exit_status, exit) = crate::common::reporting::report_fact(
        state,
        json!({
            "session_id": session_id,
            "type": "session.exited",
            "data": { "runtime_instance_id": runtime_instance_id, "reason": "quit" }
        }),
    )
    .await;

    assert_eq!(exit_status, StatusCode::OK, "{exit:?}");
    assert_eq!(
        tmux_display(&test_socket, &pane_id, "#{@pontia_session_id}"),
        ""
    );
    assert_eq!(
        tmux_display(&test_socket, &pane_id, "#{@pontia_runtime_instance_id}"),
        ""
    );
}

#[tokio::test]
async fn terminate_manually_bound_tui_without_pane_binding_is_rejected() {
    let (state, _app) = test_state().await;
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
            _app.pontia_home()
                .path()
                .join("missing.sock")
                .to_str()
                .unwrap(),
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

fn tmux_display(socket: &std::path::Path, target: &str, format: &str) -> String {
    let output = Command::new("tmux")
        .arg("-S")
        .arg(socket)
        .args(["display-message", "-p", "-t", target, format])
        .output()
        .expect("tmux display");
    assert!(output.status.success(), "tmux display should succeed");
    String::from_utf8(output.stdout)
        .expect("utf8")
        .trim()
        .to_string()
}

struct TmuxSessionGuard(std::path::PathBuf);

impl Drop for TmuxSessionGuard {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .arg("-S")
            .arg(&self.0)
            .arg("kill-server")
            .stderr(Stdio::null())
            .status();
    }
}
