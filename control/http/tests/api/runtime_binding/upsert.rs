use super::{StatusCode, Value, json, post_upsert, request_json, test_state, upsert_body};
use sqlx::Row;
#[tokio::test]
async fn pi_client_session_key_binds_the_precreated_pontia_session_without_marker_identity() {
    let (state, _app) = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();
    let session_id = "sess_precreated_pi";
    sqlx::query(
        "INSERT INTO sessions (session_id, client_type, state, metadata) VALUES (?, 'pi', 'starting', '{}')",
    )
    .bind(session_id)
    .execute(&state.db())
    .await
    .expect("precreate session");
    sqlx::query(
        r#"INSERT INTO runtime_bindings
           (session_id, runtime_kind, runtime_instance_id, launch_cwd, tmux_socket_path, tmux_pane_id, metadata)
           VALUES (?, 'pi_tui', 'rtinst_precreated', ?, '/tmp/tmux-1000/default', '%42',
                   '{"runtime_instance_id":"rtinst_precreated","binding_confirmed":false}')"#,
    )
    .bind(session_id)
    .bind(&workspace)
    .execute(&state.db())
    .await
    .expect("precreate runtime binding");
    let mut body = upsert_body(&workspace, Some("%42"));
    body["client_session_key"] = json!(session_id);

    let (status, response) = post_upsert(state.clone(), body).await;

    assert_eq!(status, StatusCode::OK, "{response:?}");
    assert_eq!(response["session"]["session_id"], session_id);
    assert_eq!(
        response["runtime"]["runtime_instance_id"],
        "rtinst_precreated"
    );
    let binding_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_bindings WHERE session_id = ? AND client_session_key = ?",
    )
    .bind(session_id)
    .bind(session_id)
    .fetch_one(&state.db())
    .await
    .expect("agent binding count");
    assert_eq!(binding_count, 1);
}

#[tokio::test]
async fn claude_client_session_key_binds_the_precreated_runtime_by_controlled_pane() {
    let (state, _app) = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();
    let session_id = "sess_precreated_claude";
    sqlx::query(
        "INSERT INTO sessions (session_id, client_type, state, metadata) VALUES (?, 'claude', 'starting', '{}')",
    )
    .bind(session_id)
    .execute(&state.db())
    .await
    .expect("precreate session");
    sqlx::query(
        r#"INSERT INTO runtime_bindings
           (session_id, runtime_kind, runtime_instance_id, launch_cwd, tmux_socket_path, tmux_pane_id, metadata)
           VALUES (?, 'claude_tui', 'rtinst_precreated_claude', ?, '/tmp/tmux-1000/default', '%52',
                   '{"runtime_instance_id":"rtinst_precreated_claude","binding_confirmed":false}')"#,
    )
    .bind(session_id)
    .bind(&workspace)
    .execute(&state.db())
    .await
    .expect("precreate runtime binding");
    let mut body = upsert_body(&workspace, Some("%52"));
    body["client_type"] = json!("claude");
    body["client_session_key"] = json!("claude_native_session");
    body["start_command"] = json!("claude");

    let (status, response) = post_upsert(state.clone(), body).await;

    assert_eq!(status, StatusCode::OK, "{response:?}");
    assert_eq!(response["session"]["session_id"], session_id);
    assert_eq!(
        response["runtime"]["runtime_instance_id"],
        "rtinst_precreated_claude"
    );
    let bound_session: String = sqlx::query_scalar(
        "SELECT session_id FROM agent_bindings WHERE client_type = 'claude' AND client_session_key = 'claude_native_session'",
    )
    .fetch_one(&state.db())
    .await
    .expect("Claude Agent binding");
    assert_eq!(bound_session, session_id);
}

#[tokio::test]
async fn fork_upsert_creates_independent_child_session_with_lineage() {
    let (state, _app) = test_state().await;
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
    let (state, app) = test_state().await;
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
    let runtime_instance_id = body["runtime"]["runtime_instance_id"]
        .as_str()
        .expect("runtime_instance_id");
    assert!(runtime_instance_id.starts_with("rtinst_"));
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
    assert_eq!(
        row.get::<String, _>("runtime_instance_id"),
        runtime_instance_id
    );
    assert_eq!(row.get::<String, _>("start_command"), "pi --approve");
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
    let expected_state_dir = app.pontia_home().path().join("state");
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

    let binding_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_bindings WHERE session_id = ? AND client_type = 'pi' AND client_session_key = 'pi_session_123'")
        .bind(session_id)
        .fetch_one(&state.db())
        .await
        .expect("agent binding count");
    assert_eq!(binding_count, 1);
    let client_session_file: Option<String> =
        sqlx::query_scalar("SELECT client_session_file FROM agent_bindings WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&state.db())
            .await
            .expect("agent binding client_session_file");
    assert_eq!(
        client_session_file.as_deref(),
        Some("/tmp/pi/session.jsonl")
    );

    let lifecycle_sources: Vec<(String, String)> =
        sqlx::query_as("SELECT event_type, source FROM events WHERE session_id = ? ORDER BY rowid")
            .bind(session_id)
            .fetch_all(&state.db())
            .await
            .expect("lifecycle event sources");
    assert_eq!(
        lifecycle_sources,
        vec![
            ("session.created".to_string(), "runtime_manager".to_string()),
            (
                "session.starting".to_string(),
                "runtime_manager".to_string()
            ),
            ("session.started".to_string(), "runtime_manager".to_string()),
        ]
    );
}

#[tokio::test]
async fn upsert_is_idempotent_for_same_pi_session_key_and_refreshes_runtime_fields() {
    let (state, _app) = test_state().await;
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

    let mut second_body = upsert_body(&workspace, Some("%42"));
    second_body["runtime_instance_id"] = first["runtime"]["runtime_instance_id"].clone();
    second_body["start_command"] = json!("pi --resume");
    let (second_status, second) = post_upsert(state.clone(), second_body).await;

    assert_eq!(second_status, StatusCode::OK, "{second:?}");
    assert_eq!(second["session"]["session_id"], first_session_id);
    assert_eq!(
        second["runtime"]["runtime_instance_id"],
        first["runtime"]["runtime_instance_id"]
    );

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
    assert_eq!(
        row.get::<String, _>("runtime_instance_id"),
        second["runtime"]["runtime_instance_id"].as_str().unwrap()
    );
    assert_eq!(row.get::<String, _>("start_command"), "pi --resume");
    assert_eq!(row.get::<String, _>("tmux_pane_id"), "%42");

    let binding_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_bindings")
        .fetch_one(&state.db())
        .await
        .expect("agent binding count");
    assert_eq!(binding_count, 1);
}

#[tokio::test]
async fn upsert_rejects_a_different_tui_while_the_bound_session_is_not_exited() {
    let (state, _app) = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();

    let (first_status, first) =
        post_upsert(state.clone(), upsert_body(&workspace, Some("%42"))).await;
    assert_eq!(first_status, StatusCode::OK, "{first:?}");
    let session_id = first["session"]["session_id"].as_str().unwrap();
    let runtime_instance_id = first["runtime"]["runtime_instance_id"].as_str().unwrap();

    let mut replacement = upsert_body(&workspace, Some("%99"));
    replacement["session_id"] = json!(session_id);
    replacement["runtime_instance_id"] = json!(runtime_instance_id);
    replacement["client_session_key"] = json!("pi_session_456");
    let (status, body) = post_upsert(state.clone(), replacement).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"]["code"], "state_conflict");
    let pane_id: String =
        sqlx::query_scalar("SELECT tmux_pane_id FROM runtime_bindings WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&state.db())
            .await
            .expect("runtime binding");
    assert_eq!(pane_id, "%42");
}

#[tokio::test]
async fn upsert_rejects_a_different_runtime_owner_while_a_turn_is_active() {
    let (state, _app) = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();

    let (first_status, first) =
        post_upsert(state.clone(), upsert_body(&workspace, Some("%42"))).await;
    assert_eq!(first_status, StatusCode::OK, "{first:?}");
    let session_id = first["session"]["session_id"].as_str().unwrap();
    let runtime_instance_id = first["runtime"]["runtime_instance_id"].as_str().unwrap();
    let (started_status, started) = request_json(
        state.clone(),
        "POST",
        "/internal/v1/events",
        Some(json!({
            "session_id": session_id,
            "type": "turn.started",
            "data": { "runtime_instance_id": runtime_instance_id }
        })),
    )
    .await;
    assert_eq!(started_status, StatusCode::OK, "{started:?}");

    let mut replacement = upsert_body(&workspace, Some("%99"));
    replacement["session_id"] = json!(session_id);
    let (replacement_status, replacement_body) = post_upsert(state.clone(), replacement).await;

    assert_eq!(
        replacement_status,
        StatusCode::CONFLICT,
        "{replacement_body:?}"
    );
    assert_eq!(replacement_body["error"]["code"], "state_conflict");
    let row = sqlx::query(
        "SELECT runtime_instance_id, tmux_pane_id FROM runtime_bindings WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_one(&state.db())
    .await
    .expect("runtime binding");
    assert_eq!(
        row.get::<String, _>("runtime_instance_id"),
        runtime_instance_id
    );
    assert_eq!(row.get::<String, _>("tmux_pane_id"), "%42");

    let mut different_pane_refresh = upsert_body(&workspace, Some("%77"));
    different_pane_refresh["session_id"] = json!(session_id);
    different_pane_refresh["runtime_instance_id"] = json!(runtime_instance_id);
    let (refresh_status, refresh_body) = post_upsert(state.clone(), different_pane_refresh).await;
    assert_eq!(refresh_status, StatusCode::CONFLICT, "{refresh_body:?}");
    assert_eq!(refresh_body["error"]["code"], "state_conflict");
}

#[tokio::test]
async fn concurrent_first_upserts_for_one_pi_session_key_create_once_without_overwriting() {
    let (state, _app) = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();
    let first_request = upsert_body(&workspace, Some("%42"));
    let second_request = upsert_body(&workspace, Some("%42"));

    let (first, second) = tokio::join!(
        post_upsert(state.clone(), first_request),
        post_upsert(state.clone(), second_request)
    );

    assert_eq!(first.0, StatusCode::OK, "{:?}", first.1);
    assert_eq!(second.0, StatusCode::CONFLICT, "{:?}", second.1);
    assert_eq!(second.1["error"]["code"], "state_conflict");
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
    let (state, _app) = test_state().await;
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

    let retry = upsert_body(&workspace, Some("%99"));
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
    assert_eq!(
        runtime_instance_id,
        first["runtime"]["runtime_instance_id"].as_str().unwrap()
    );
}

#[tokio::test]
async fn upsert_rejects_a_request_without_tmux_binding() {
    let (state, _app) = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();

    let (status, body) = post_upsert(state.clone(), upsert_body(&workspace, None)).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"]["code"], "state_conflict");
    assert_eq!(
        body["error"]["message"],
        "runtime binding upsert requires tmux"
    );
    let session_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&state.db())
        .await
        .expect("session count");
    assert_eq!(session_count, 0);
}
