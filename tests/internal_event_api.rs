use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia::{
    application::{AppState, EventIngestService},
    domain::{SessionState, TurnState},
    storage::sqlite::{connect_sqlite, run_migrations},
    transport::http,
};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("m2.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    AppState {
        db,
        external_api_token: None,
        graph: Default::default(),
        workspace_browser: Default::default(),
        dashboard: pontia::transport::http::dashboard::ResolvedDashboard::local_default(),
        shutdown: Default::default(),
        volatile_events: Default::default(),
        git_refresh: Default::default(),
    }
}

fn event_body(
    event_id: &str,
    event_type: &str,
    session_id: &str,
    turn_id: Option<&str>,
    seq: i64,
) -> Value {
    json!({
        "event_id": event_id,
        "session_id": session_id,
        "turn_id": turn_id,
        "source": "agent_adapter",
        "client_type": "generic",
        "type": event_type,
        "time": "2026-04-24T12:00:00Z",
        "seq": seq,
        "payload": {}
    })
}

async fn post_event(state: AppState, body: Value) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/v1/events")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
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
async fn internal_event_api_accepts_session_event_and_updates_projection() {
    let state = test_state().await;

    let (status, body) = post_event(
        state.clone(),
        event_body("evt_m2_1", "session.created", "sess_m2_1", None, 1),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["accepted"], true);
    assert_eq!(body["duplicate"], false);
    assert_eq!(body["state_version"], 1);
    assert_eq!(body["warnings"], json!([]));

    let service = EventIngestService::new(state.db);
    let session = service
        .get_session("sess_m2_1")
        .await
        .expect("session query")
        .expect("session projection");
    assert_eq!(session.state, SessionState::Created);
}

#[tokio::test]
async fn internal_event_api_accepts_turn_events_and_updates_projection() {
    let state = test_state().await;

    let created = event_body("evt_m2_2_created", "session.created", "sess_m2_2", None, 1);
    post_event(state.clone(), created).await;
    let mut ready = event_body("evt_m2_2", "session.ready", "sess_m2_2", None, 2);
    ready["source"] = json!("agent_client");
    ready["payload"] = json!({"runtime_instance_id":"rtinst_m2_2"});
    post_event(state.clone(), ready).await;
    let (status, body) = post_event(
        state.clone(),
        event_body(
            "evt_m2_3",
            "turn.started",
            "sess_m2_2",
            Some("turn_m2_1"),
            3,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["turn_id"], "turn_m2_1");
    assert_eq!(body["state_version"], 3);

    let service = EventIngestService::new(state.db);
    let session = service.get_session("sess_m2_2").await.unwrap().unwrap();
    let turn = service.get_turn("turn_m2_1").await.unwrap().unwrap();
    assert_eq!(session.state, SessionState::Busy);
    assert_eq!(turn.state, TurnState::Running);
}

#[tokio::test]
async fn internal_event_api_rejects_missing_required_schema_fields() {
    let state = test_state().await;
    let mut event = event_body(
        "evt_m2_missing",
        "session.created",
        "sess_m2_missing",
        None,
        1,
    );
    event.as_object_mut().unwrap().remove("event_id");

    let (status, body) = post_event(state, event).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn internal_event_api_rejects_unknown_event_type() {
    let state = test_state().await;
    let (status, body) = post_event(
        state,
        event_body("evt_m2_4", "approval.requested", "sess_m2_3", None, 1),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn internal_event_api_rejects_turn_event_without_turn_id() {
    let state = test_state().await;
    let (status, body) = post_event(
        state,
        event_body("evt_m2_5", "turn.completed", "sess_m2_4", None, 1),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn internal_event_api_accepts_session_message_updated_without_changing_projection() {
    let state = test_state().await;
    let (created_status, _) = post_event(
        state.clone(),
        event_body(
            "evt_m2_message_updated_created",
            "session.created",
            "sess_m2_message_updated",
            None,
            1,
        ),
    )
    .await;
    assert_eq!(created_status, StatusCode::OK);

    let mut event = event_body(
        "evt_m2_message_updated",
        "session.message_updated",
        "sess_m2_message_updated",
        None,
        2,
    );
    event["source"] = json!("agent_client");
    event["payload"] = json!({"reason":"update"});

    let (status, body) = post_event(state.clone(), event).await;

    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["accepted"], true);
    assert_eq!(body["turn_id"], Value::Null);

    let event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE session_id = ?")
        .bind("sess_m2_message_updated")
        .fetch_one(&state.db)
        .await
        .expect("event count");
    assert_eq!(event_count, 1);

    let service = EventIngestService::new(state.db);
    let session = service
        .get_session("sess_m2_message_updated")
        .await
        .expect("session query")
        .expect("session projection");
    assert_eq!(session.state, SessionState::Created);
    assert_eq!(session.state_version, 1);
}

#[tokio::test]
async fn internal_event_api_accepts_agent_client_ready_with_runtime_instance_id_for_existing_session()
 {
    let state = test_state().await;
    let launch_cwd = tempfile::tempdir().expect("workspace");
    let launch_cwd = launch_cwd
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let mut created = event_body(
        "evt_m2_ready_created",
        "session.created",
        "sess_m2_ready",
        None,
        1,
    );
    created["source"] = json!("external_api");
    created["client_type"] = json!("pi");
    post_event(state.clone(), created).await;
    sqlx::query(
        "INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_ref, metadata) VALUES (?, 'tmux', 'pontia-test', ?)",
    )
    .bind("sess_m2_ready")
    .bind(json!({"runtime_instance_id":"rtinst_test", "workspace": launch_cwd.display().to_string()}).to_string())
    .execute(&state.db)
    .await
    .expect("runtime binding");

    let mut event = event_body("evt_m2_ready", "session.ready", "sess_m2_ready", None, 2);
    event["source"] = json!("agent_client");
    event["client_type"] = json!("pi");
    event["payload"] = json!({
        "runtime_instance_id":"rtinst_test",
        "client_session_key":"pi_session_123",
        "client_session_file":"/diagnostic/session.jsonl",
        "client_cwd":"/diagnostic/cwd"
    });

    let (status, body) = post_event(state.clone(), event).await;

    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["accepted"], true);

    let row = sqlx::query(
        "SELECT session_id, client_type, launch_cwd, client_session_key, metadata FROM agent_bindings WHERE session_id = ?",
    )
    .bind("sess_m2_ready")
    .fetch_one(&state.db)
    .await
    .expect("agent binding row");
    let session_id: String = sqlx::Row::try_get(&row, "session_id").unwrap();
    let client_type: String = sqlx::Row::try_get(&row, "client_type").unwrap();
    let stored_launch_cwd: String = sqlx::Row::try_get(&row, "launch_cwd").unwrap();
    let client_session_key: String = sqlx::Row::try_get(&row, "client_session_key").unwrap();
    let metadata: String = sqlx::Row::try_get(&row, "metadata").unwrap();
    let metadata: Value = serde_json::from_str(&metadata).unwrap();
    assert_eq!(session_id, "sess_m2_ready");
    assert_eq!(client_type, "pi");
    assert_eq!(stored_launch_cwd, launch_cwd.display().to_string());
    assert_eq!(client_session_key, "pi_session_123");
    assert_eq!(metadata["client_session_file"], "/diagnostic/session.jsonl");
    assert_eq!(metadata["client_cwd"], "/diagnostic/cwd");
}

#[tokio::test]
async fn internal_event_api_ready_agent_binding_is_idempotent_for_retries() {
    let state = test_state().await;
    let launch_cwd = tempfile::tempdir().expect("workspace");
    let launch_cwd = launch_cwd
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let mut created = event_body(
        "evt_m2_ready_retry_created",
        "session.created",
        "sess_m2_ready_retry",
        None,
        1,
    );
    created["source"] = json!("external_api");
    created["client_type"] = json!("pi");
    post_event(state.clone(), created).await;
    sqlx::query(
        "INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_ref, metadata) VALUES (?, 'tmux', 'pontia-test', ?)",
    )
    .bind("sess_m2_ready_retry")
    .bind(json!({"runtime_instance_id":"rtinst_retry", "workspace": launch_cwd.display().to_string()}).to_string())
    .execute(&state.db)
    .await
    .expect("runtime binding");

    for event_id in ["evt_m2_ready_retry_1", "evt_m2_ready_retry_2"] {
        let mut event = event_body(event_id, "session.ready", "sess_m2_ready_retry", None, 2);
        event["source"] = json!("agent_client");
        event["client_type"] = json!("pi");
        event["payload"] =
            json!({"runtime_instance_id":"rtinst_retry", "client_session_key":"pi_retry"});
        let (status, body) = post_event(state.clone(), event).await;
        assert_eq!(status, StatusCode::OK, "{body:?}");
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_bindings WHERE session_id = ?")
        .bind("sess_m2_ready_retry")
        .fetch_one(&state.db)
        .await
        .expect("agent binding count");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn internal_event_api_rejects_agent_client_ready_for_unknown_session() {
    let state = test_state().await;
    let mut event = event_body(
        "evt_m2_unknown_ready",
        "session.ready",
        "sess_m2_unknown_ready",
        None,
        1,
    );
    event["source"] = json!("agent_client");
    event["client_type"] = json!("pi");
    event["payload"] =
        json!({"runtime_instance_id":"rtinst_test", "client_session_key":"pi_unknown"});

    let (status, body) = post_event(state.clone(), event).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unknown session")
    );
    let service = EventIngestService::new(state.db);
    assert!(
        service
            .get_session("sess_m2_unknown_ready")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn internal_event_api_rejects_agent_client_ready_without_runtime_instance_id() {
    let state = test_state().await;
    let mut event = event_body("evt_m2_bad_ready", "session.ready", "sess_m2_bad", None, 1);
    event["source"] = json!("agent_client");
    event["client_type"] = json!("pi");

    let (status, body) = post_event(state, event).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("runtime_instance_id")
    );
}

#[tokio::test]
async fn internal_event_api_rejects_pi_agent_client_ready_without_client_session_key() {
    let state = test_state().await;
    let mut created = event_body(
        "evt_m2_bad_ready_key_created",
        "session.created",
        "sess_m2_bad_key",
        None,
        1,
    );
    created["source"] = json!("external_api");
    created["client_type"] = json!("pi");
    post_event(state.clone(), created).await;

    let mut event = event_body(
        "evt_m2_bad_ready_key",
        "session.ready",
        "sess_m2_bad_key",
        None,
        2,
    );
    event["source"] = json!("agent_client");
    event["client_type"] = json!("pi");
    event["payload"] = json!({"runtime_instance_id":"rtinst_bad_key"});

    let (status, body) = post_event(state, event).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("client_session_key")
    );
}

#[tokio::test]
async fn internal_event_api_is_idempotent_for_duplicate_event_id() {
    let state = test_state().await;
    let event = event_body("evt_m2_same", "session.created", "sess_m2_5", None, 1);

    let first = post_event(state.clone(), event.clone()).await;
    let second = post_event(state.clone(), event).await;

    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(second.0, StatusCode::OK);
    assert_eq!(second.1["accepted"], true);
    assert_eq!(second.1["duplicate"], true);
    assert_eq!(second.1["state_version"], 1);

    let service = EventIngestService::new(state.db);
    assert_eq!(service.list_events("sess_m2_5").await.unwrap().len(), 1);
}

#[tokio::test]
async fn internal_event_api_maps_domain_conflicts_to_conflict() {
    let state = test_state().await;

    post_event(
        state.clone(),
        event_body("evt_m2_6", "session.ready", "sess_m2_6", None, 1),
    )
    .await;
    post_event(
        state.clone(),
        event_body(
            "evt_m2_7",
            "turn.started",
            "sess_m2_6",
            Some("turn_m2_2"),
            2,
        ),
    )
    .await;
    let (status, body) = post_event(
        state,
        event_body(
            "evt_m2_8",
            "turn.started",
            "sess_m2_6",
            Some("turn_m2_3"),
            3,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "state_conflict");
}

#[tokio::test]
async fn internal_event_api_rejects_large_payloads() {
    let state = test_state().await;
    let mut event = event_body("evt_m2_9", "turn.output", "sess_m2_7", Some("turn_m2_4"), 1);
    event["payload"] = json!({ "content": "x".repeat(70_000) });

    let (status, body) = post_event(state, event).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn internal_event_api_accepts_sequence_gaps_with_warnings() {
    let state = test_state().await;

    post_event(
        state.clone(),
        event_body("evt_m2_10", "session.created", "sess_m2_8", None, 1),
    )
    .await;
    let (status, body) = post_event(
        state,
        event_body("evt_m2_11", "session.ready", "sess_m2_8", None, 3),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["accepted"], true);
    assert_eq!(body["warnings"].as_array().unwrap().len(), 1);
    assert!(
        body["warnings"][0]
            .as_str()
            .unwrap()
            .contains("sequence gap")
    );
}
