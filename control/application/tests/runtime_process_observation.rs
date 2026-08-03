use std::{
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use pontia_application::RuntimeObservationService;
use pontia_runtime::GenericRuntimeManager;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;

#[tokio::test]
async fn missing_bound_agent_process_projects_session_exited_after_confirmation() {
    let tmux_session = format!("pontia_test_observation_{}", std::process::id());
    let status = Command::new("tmux")
        .args(["new-session", "-d", "-s", &tmux_session, "sleep 60"])
        .stderr(Stdio::null())
        .status()
        .expect("spawn tmux session");
    assert!(status.success());

    let socket_path = tmux_value(&tmux_session, "#{socket_path}");
    let pane_id = tmux_value(&tmux_session, "#{pane_id}");
    let fingerprint = (0..50)
        .find_map(|_| {
            let fingerprint = GenericRuntimeManager.capture_tmux_process_fingerprint(
                &socket_path,
                &pane_id,
                &["sleep"],
            );
            if fingerprint.is_none() {
                thread::sleep(Duration::from_millis(20));
            }
            fingerprint
        })
        .expect("capture sleep fingerprint");

    let temp = tempfile::tempdir().expect("tempdir");
    let database_url = format!(
        "sqlite://{}?mode=rwc",
        temp.path().join("test.db").display()
    );
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    sqlx::query(
        "INSERT INTO sessions (session_id, client_type, state) VALUES ('sess_observed', 'pi', 'idle')",
    )
    .execute(&db)
    .await
    .expect("insert session");
    sqlx::query(
        r#"INSERT INTO runtime_bindings (
               session_id, runtime_kind, runtime_instance_id,
               tmux_socket_path, tmux_pane_id, metadata
           ) VALUES (?, 'pi_tui', ?, ?, ?, ?)"#,
    )
    .bind("sess_observed")
    .bind("rtinst_observed")
    .bind(&socket_path)
    .bind(&pane_id)
    .bind(json!({ "tmux_process_fingerprint": fingerprint }).to_string())
    .execute(&db)
    .await
    .expect("insert binding");

    let _ = Command::new("tmux")
        .args(["kill-session", "-t", &tmux_session])
        .stderr(Stdio::null())
        .status();

    RuntimeObservationService::new(db.clone())
        .sweep_active_tmux_sessions()
        .await
        .expect("sweep runtime bindings");

    let state: String = sqlx::query_scalar("SELECT state FROM sessions WHERE session_id = ?")
        .bind("sess_observed")
        .fetch_one(&db)
        .await
        .expect("load session state");
    assert_eq!(state, "exited");

    let source: String = sqlx::query_scalar(
        "SELECT source FROM events WHERE session_id = ? AND event_type = 'session.exited'",
    )
    .bind("sess_observed")
    .fetch_one(&db)
    .await
    .expect("load exit event");
    assert_eq!(source, "runtime_manager");
}

fn tmux_value(session: &str, format: &str) -> String {
    let output = Command::new("tmux")
        .args(["display-message", "-p", "-t", session, format])
        .output()
        .expect("query tmux");
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .expect("tmux output utf8")
        .trim()
        .to_string()
}
