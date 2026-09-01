use super::{AppState, StatusCode, json, post_upsert, request_json, test_state, upsert_body};
#[tokio::test]
async fn current_runtime_exit_abandons_its_active_turn() {
    let (state, _app) = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();

    let (upsert_status, upsert) =
        post_upsert(state.clone(), upsert_body(&workspace, Some("%42"))).await;
    assert_eq!(upsert_status, StatusCode::OK, "{upsert:?}");
    let session_id = upsert["session"]["session_id"].as_str().unwrap();
    let runtime_instance_id = upsert["runtime"]["runtime_instance_id"].as_str().unwrap();
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
    let turn_id = started["turn_id"].as_str().unwrap();

    let (exit_status, exit) = request_json(
        state.clone(),
        "POST",
        "/internal/v1/events",
        Some(json!({
            "session_id": session_id,
            "type": "session.exited",
            "data": {
                "runtime_instance_id": runtime_instance_id,
                "reason": "process_exit"
            }
        })),
    )
    .await;
    assert_eq!(exit_status, StatusCode::OK, "{exit:?}");

    let (turn_status, turn_body) = request_json(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}"),
        None,
    )
    .await;
    assert_eq!(turn_status, StatusCode::OK, "{turn_body:?}");
    assert_eq!(turn_body["data"]["turn"]["state"], "abandoned");
    assert_eq!(
        turn_body["data"]["turn"]["metadata"]["terminal_provenance"]["reason"],
        "session_exited_without_terminal_fact"
    );
    assert_eq!(
        turn_body["data"]["turn"]["metadata"]["terminal_provenance"]["event_type"],
        "session.exited"
    );
}

#[tokio::test]
async fn stale_runtime_exit_cannot_exit_the_current_runtime_session() {
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
    let runtime_a = first["runtime"]["runtime_instance_id"].as_str().unwrap();
    let (exit_status, exit_body) = request_json(
        state.clone(),
        "POST",
        "/internal/v1/events",
        Some(json!({
            "session_id": session_id,
            "type": "session.exited",
            "data": { "runtime_instance_id": runtime_a, "reason": "quit" }
        })),
    )
    .await;
    assert_eq!(exit_status, StatusCode::OK, "{exit_body:?}");

    let mut replacement = upsert_body(&workspace, Some("%99"));
    replacement["session_id"] = json!(session_id);
    let (second_status, second) = post_upsert(state.clone(), replacement).await;
    assert_eq!(second_status, StatusCode::OK, "{second:?}");
    let runtime_b = second["runtime"]["runtime_instance_id"].as_str().unwrap();
    assert_ne!(runtime_b, runtime_a);

    let (stale_status, stale_body) = request_json(
        state.clone(),
        "POST",
        "/internal/v1/events",
        Some(json!({
            "session_id": session_id,
            "type": "session.exited",
            "data": { "runtime_instance_id": runtime_a, "reason": "stale_exit" }
        })),
    )
    .await;
    assert_eq!(stale_status, StatusCode::BAD_REQUEST, "{stale_body:?}");

    let (session_status, session_body) = request_json(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}"),
        None,
    )
    .await;
    assert_eq!(session_status, StatusCode::OK, "{session_body:?}");
    assert_ne!(session_body["data"]["session"]["state"], "exited");
}

#[tokio::test]
async fn retrying_an_old_terminal_fact_does_not_end_the_current_turn() {
    let (state, _app) = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let workspace = workspace.display().to_string();
    let (upsert_status, upsert) =
        post_upsert(state.clone(), upsert_body(&workspace, Some("%42"))).await;
    assert_eq!(upsert_status, StatusCode::OK, "{upsert:?}");
    let session_id = upsert["session"]["session_id"].as_str().unwrap();
    let runtime_instance_id = upsert["runtime"]["runtime_instance_id"].as_str().unwrap();

    let (first_started_status, first_started) = request_json(
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
    assert_eq!(first_started_status, StatusCode::OK, "{first_started:?}");
    let first_turn_id = first_started["turn_id"].as_str().unwrap();
    let (completed_status, completed) = request_json(
        state.clone(),
        "POST",
        "/internal/v1/events",
        Some(json!({
            "session_id": session_id,
            "turn_id": first_turn_id,
            "type": "turn.completed",
            "data": {}
        })),
    )
    .await;
    assert_eq!(completed_status, StatusCode::OK, "{completed:?}");

    let (second_started_status, second_started) = request_json(
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
    assert_eq!(second_started_status, StatusCode::OK, "{second_started:?}");
    let second_turn_id = second_started["turn_id"].as_str().unwrap();

    let (retry_status, retry) = request_json(
        state.clone(),
        "POST",
        "/internal/v1/events",
        Some(json!({
            "session_id": session_id,
            "turn_id": first_turn_id,
            "type": "turn.completed",
            "data": {}
        })),
    )
    .await;
    assert_eq!(retry_status, StatusCode::OK, "{retry:?}");

    let (session_status, session_body) = request_json(
        state.clone(),
        "GET",
        &format!("/external/v1/sessions/{session_id}"),
        None,
    )
    .await;
    assert_eq!(session_status, StatusCode::OK, "{session_body:?}");
    assert_eq!(
        session_body["data"]["session"]["current_turn_id"],
        second_turn_id
    );
    let (turn_status, turn_body) = request_json(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}/turns/{second_turn_id}"),
        None,
    )
    .await;
    assert_eq!(turn_status, StatusCode::OK, "{turn_body:?}");
    assert_eq!(turn_body["data"]["turn"]["state"], "running");
}

#[tokio::test]
async fn upsert_existing_exited_pi_session_records_resume_lifecycle() {
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
    let session_id = first["session"]["session_id"].as_str().unwrap().to_string();

    let (exit_status, exit) = request_json(
        state.clone(),
        "POST",
        "/internal/v1/events",
        Some(json!({
            "session_id": session_id,
            "type": "session.exited",
            "data": {
                "runtime_instance_id": first["runtime"]["runtime_instance_id"],
                "reason": "quit"
            }
        })),
    )
    .await;
    assert_eq!(exit_status, StatusCode::OK, "{exit:?}");

    let second_body = upsert_body(&workspace, Some("%99"));
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

    let lifecycle_events: Vec<(String, String)> = sqlx::query_as(
        "SELECT event_type, source FROM events WHERE session_id = ? ORDER BY rowid DESC LIMIT 2",
    )
    .bind(&session_id)
    .fetch_all(&state.db())
    .await
    .expect("lifecycle events");
    assert_eq!(
        lifecycle_events,
        vec![
            ("session.started".to_string(), "runtime_manager".to_string()),
            (
                "session.resuming".to_string(),
                "runtime_manager".to_string()
            ),
        ]
    );
}

#[tokio::test]
async fn repeated_webui_resume_of_manually_bound_pi_tui_does_not_persist_session_id_argument() {
    let (state, _app) = test_state().await;
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

    let first_resumed_runtime_instance_id = exit_and_resume(
        state.clone(),
        session_id,
        upsert["runtime"]["runtime_instance_id"]
            .as_str()
            .expect("initial runtime instance id"),
    )
    .await;
    assert_persisted_start_command(&state, session_id, "pi").await;

    exit_and_resume(
        state.clone(),
        session_id,
        &first_resumed_runtime_instance_id,
    )
    .await;
    assert_persisted_start_command(&state, session_id, "pi").await;
}

async fn exit_and_resume(state: AppState, session_id: &str, runtime_instance_id: &str) -> String {
    let (exit_status, exit) = request_json(
        state.clone(),
        "POST",
        "/internal/v1/events",
        Some(json!({
            "session_id": session_id,
            "type": "session.exited",
            "data": {
                "runtime_instance_id": runtime_instance_id,
                "reason": "quit"
            }
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

    sqlx::query_scalar("SELECT runtime_instance_id FROM runtime_bindings WHERE session_id = ?")
        .bind(session_id)
        .fetch_one(&state.db())
        .await
        .expect("resumed runtime instance id")
}

async fn assert_persisted_start_command(state: &AppState, session_id: &str, expected: &str) {
    let start_command: String =
        sqlx::query_scalar("SELECT start_command FROM runtime_bindings WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&state.db())
            .await
            .expect("runtime start command");
    assert_eq!(start_command, expected);
}
