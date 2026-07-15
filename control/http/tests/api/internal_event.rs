use crate::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_application::{AppState, EventIngestService};
use pontia_core::domain::{SessionState, TurnState};
use pontia_http as http;
use serde_json::{Value, json};
use tower::ServiceExt;

async fn test_state() -> AppState {
    TestApp::builder()
        .database_name("internal_event.db")
        .external_api_token(None)
        .build_state()
        .await
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
        event_body(
            "evt_internal_event_1",
            "session.created",
            "sess_internal_event_1",
            None,
            1,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["accepted"], true);
    assert_eq!(body["duplicate"], false);
    assert_eq!(body["state_version"], 1);
    assert_eq!(body["warnings"], json!([]));

    let service = EventIngestService::new(state.db());
    let session = service
        .get_session("sess_internal_event_1")
        .await
        .expect("session query")
        .expect("session projection");
    assert_eq!(session.state, SessionState::Created);
}

#[tokio::test]
async fn internal_event_api_accepts_turn_events_and_updates_projection() {
    let state = test_state().await;

    let created = event_body(
        "evt_internal_event_2_created",
        "session.created",
        "sess_internal_event_2",
        None,
        1,
    );
    post_event(state.clone(), created).await;
    let mut ready = event_body(
        "evt_internal_event_2",
        "session.ready",
        "sess_internal_event_2",
        None,
        2,
    );
    ready["source"] = json!("agent_client");
    ready["payload"] = json!({"runtime_instance_id":"rtinst_internal_event_2"});
    post_event(state.clone(), ready).await;
    let (status, body) = post_event(
        state.clone(),
        event_body(
            "evt_internal_event_3",
            "turn.started",
            "sess_internal_event_2",
            Some("turn_internal_event_1"),
            3,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["turn_id"], "turn_internal_event_1");
    assert_eq!(body["state_version"], 3);

    let service = EventIngestService::new(state.db());
    let session = service
        .get_session("sess_internal_event_2")
        .await
        .unwrap()
        .unwrap();
    let turn = service
        .get_turn("turn_internal_event_1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(session.state, SessionState::Busy);
    assert_eq!(turn.state, TurnState::Running);
}

#[tokio::test]
async fn internal_event_api_rejects_missing_required_schema_fields() {
    let state = test_state().await;
    let mut event = event_body(
        "evt_internal_event_missing",
        "session.created",
        "sess_internal_event_missing",
        None,
        1,
    );
    event.as_object_mut().unwrap().remove("event_id");

    let (status, body) = post_event(state, event).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn internal_event_api_rejects_pontia_owned_turn_index() {
    let state = test_state().await;
    let mut event = event_body(
        "evt_internal_event_owned_index",
        "turn.started",
        "sess_internal_event_owned_index",
        Some("turn_owned_index"),
        1,
    );
    event["turn_index"] = json!(42);

    let (status, body) = post_event(state, event).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Pontia-owned")
    );
}

#[tokio::test]
async fn internal_event_api_rejects_unknown_event_type() {
    let state = test_state().await;
    let (status, body) = post_event(
        state,
        event_body(
            "evt_internal_event_4",
            "approval.requested",
            "sess_internal_event_3",
            None,
            1,
        ),
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
        event_body(
            "evt_internal_event_5",
            "turn.completed",
            "sess_internal_event_4",
            None,
            1,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn internal_event_api_accepts_context_usage_and_updates_session_metadata_only() {
    let state = test_state().await;
    let mut created = event_body(
        "evt_internal_event_context_created",
        "session.created",
        "sess_internal_event_context",
        None,
        1,
    );
    created["payload"] = json!({"metadata":{"purpose":"context-test"}});
    assert_eq!(post_event(state.clone(), created).await.0, StatusCode::OK);
    assert_eq!(
        post_event(
            state.clone(),
            event_body(
                "evt_internal_event_context_ready",
                "session.ready",
                "sess_internal_event_context",
                None,
                2
            ),
        )
        .await
        .0,
        StatusCode::OK
    );

    let mut event = event_body(
        "evt_internal_event_context_usage",
        "session.context_usage_updated",
        "sess_internal_event_context",
        Some("turn_internal_event_context"),
        3,
    );
    event["source"] = json!("agent_client");
    event["client_type"] = json!("generic");
    event["payload"] = json!({
        "context_usage": {
            "used_tokens": 42000,
            "max_tokens": 128000,
            "remaining_tokens": 86000,
            "usage_ratio": 0.328125,
            "input_tokens": 40000,
            "output_tokens": 2000,
            "cache_tokens": 1000,
            "confidence": "exact"
        },
        "model": "example-model"
    });

    let (status, body) = post_event(state.clone(), event).await;

    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["turn_id"], "turn_internal_event_context");

    let service = EventIngestService::new(state.db());
    let session = service
        .get_session("sess_internal_event_context")
        .await
        .expect("session query")
        .expect("session projection");
    assert_eq!(session.state, SessionState::Idle);
    assert_eq!(session.current_turn_id, None);
    assert_eq!(session.metadata["purpose"], "context-test");
    assert_eq!(session.metadata["context_usage"]["used_tokens"], 42000);
    assert_eq!(session.metadata["context_usage"]["max_tokens"], 128000);
    assert_eq!(session.metadata["context_usage"]["confidence"], "exact");
    assert_eq!(session.metadata["model"], "example-model");
    assert!(session.metadata["context_usage"].get("model").is_none());
    assert_eq!(
        session.metadata["context_usage"]["observed_at"],
        "2026-04-24T12:00:00Z"
    );
}

#[tokio::test]
async fn internal_event_api_merges_partial_context_usage_without_clearing_existing_values() {
    let state = test_state().await;
    let mut created = event_body(
        "evt_internal_event_context_merge_created",
        "session.created",
        "sess_internal_event_context_merge",
        None,
        1,
    );
    created["payload"] = json!({"metadata":{"purpose":"context-merge-test"}});
    assert_eq!(post_event(state.clone(), created).await.0, StatusCode::OK);

    let mut full = event_body(
        "evt_internal_event_context_merge_full",
        "session.context_usage_updated",
        "sess_internal_event_context_merge",
        Some("turn_internal_event_context_merge"),
        2,
    );
    full["payload"] = json!({
        "context_usage": {
            "used_tokens": 6037,
            "max_tokens": 128000,
            "remaining_tokens": 121963,
            "usage_ratio": 0.04716,
            "confidence": "estimated"
        },
        "model": "ctx-model"
    });
    assert_eq!(post_event(state.clone(), full).await.0, StatusCode::OK);

    let mut partial = event_body(
        "evt_internal_event_context_merge_partial",
        "session.context_usage_updated",
        "sess_internal_event_context_merge",
        Some("turn_internal_event_context_merge"),
        3,
    );
    partial["payload"] = json!({
        "context_usage": {
            "used_tokens": null,
            "max_tokens": null,
            "input_tokens": 386,
            "output_tokens": 19,
            "cache_tokens": 5632,
            "confidence": "estimated"
        },
        "model": null
    });
    let (status, body) = post_event(state.clone(), partial).await;
    assert_eq!(status, StatusCode::OK, "{body:?}");

    let service = EventIngestService::new(state.db());
    let session = service
        .get_session("sess_internal_event_context_merge")
        .await
        .expect("session query")
        .expect("session projection");
    assert_eq!(session.metadata["context_usage"]["used_tokens"], 6037);
    assert_eq!(session.metadata["context_usage"]["max_tokens"], 128000);
    assert_eq!(
        session.metadata["context_usage"]["remaining_tokens"],
        121963
    );
    assert_eq!(session.metadata["context_usage"]["usage_ratio"], 0.04716);
    assert_eq!(session.metadata["context_usage"]["input_tokens"], 386);
    assert_eq!(session.metadata["context_usage"]["output_tokens"], 19);
    assert_eq!(session.metadata["context_usage"]["cache_tokens"], 5632);
    assert_eq!(session.metadata["model"], "ctx-model");
}

#[tokio::test]
async fn internal_event_api_rejects_invalid_context_usage_values() {
    let state = test_state().await;
    let invalid_cases = [
        (
            "evt_internal_event_context_negative",
            json!({"context_usage":{"used_tokens":-1,"confidence":"exact"}}),
        ),
        (
            "evt_internal_event_context_ratio",
            json!({"context_usage":{"usage_ratio":1.5,"confidence":"exact"}}),
        ),
        (
            "evt_internal_event_context_confidence",
            json!({"context_usage":{"used_tokens":1,"confidence":"approximate"}}),
        ),
        (
            "evt_internal_event_context_missing_object",
            json!({"context_usage":null}),
        ),
        (
            "evt_internal_event_context_model_nested",
            json!({"context_usage":{"used_tokens":1,"model":"nested"}}),
        ),
        (
            "evt_internal_event_context_model_type",
            json!({"context_usage":{"used_tokens":1},"model":42}),
        ),
    ];

    for (event_id, payload) in invalid_cases {
        let mut event = event_body(
            event_id,
            "session.context_usage_updated",
            "sess_internal_event_context_invalid",
            None,
            1,
        );
        event["source"] = json!("agent_client");
        event["payload"] = payload;

        let (status, body) = post_event(state.clone(), event).await;

        assert_eq!(status, StatusCode::BAD_REQUEST, "{event_id}: {body:?}");
        assert_eq!(body["error"]["code"], "invalid_request");
    }
}

#[tokio::test]
async fn internal_event_api_accepts_session_message_updated_without_changing_projection() {
    let state = test_state().await;
    let (created_status, _) = post_event(
        state.clone(),
        event_body(
            "evt_internal_event_message_updated_created",
            "session.created",
            "sess_internal_event_message_updated",
            None,
            1,
        ),
    )
    .await;
    assert_eq!(created_status, StatusCode::OK);

    let mut event = event_body(
        "evt_internal_event_message_updated",
        "session.message_updated",
        "sess_internal_event_message_updated",
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
        .bind("sess_internal_event_message_updated")
        .fetch_one(&state.db())
        .await
        .expect("event count");
    assert_eq!(event_count, 1);

    let service = EventIngestService::new(state.db());
    let session = service
        .get_session("sess_internal_event_message_updated")
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
        "evt_internal_event_ready_created",
        "session.created",
        "sess_internal_event_ready",
        None,
        1,
    );
    created["source"] = json!("external_api");
    created["client_type"] = json!("pi");
    post_event(state.clone(), created).await;
    sqlx::query(
        "INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, launch_cwd, metadata) VALUES (?, 'tmux', 'rtinst_test', ?, ?)",
    )
    .bind("sess_internal_event_ready")
    .bind(launch_cwd.display().to_string())
    .bind(json!({"runtime_instance_id":"rtinst_test", "workspace": launch_cwd.display().to_string()}).to_string())
    .execute(&state.db())
    .await
    .expect("runtime binding");

    let mut event = event_body(
        "evt_internal_event_ready",
        "session.ready",
        "sess_internal_event_ready",
        None,
        2,
    );
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
    .bind("sess_internal_event_ready")
    .fetch_one(&state.db())
    .await
    .expect("agent binding row");
    let session_id: String = sqlx::Row::try_get(&row, "session_id").unwrap();
    let client_type: String = sqlx::Row::try_get(&row, "client_type").unwrap();
    let stored_launch_cwd: String = sqlx::Row::try_get(&row, "launch_cwd").unwrap();
    let client_session_key: String = sqlx::Row::try_get(&row, "client_session_key").unwrap();
    let metadata: String = sqlx::Row::try_get(&row, "metadata").unwrap();
    let metadata: Value = serde_json::from_str(&metadata).unwrap();
    assert_eq!(session_id, "sess_internal_event_ready");
    assert_eq!(client_type, "pi");
    assert_eq!(stored_launch_cwd, launch_cwd.display().to_string());
    assert_eq!(client_session_key, "pi_session_123");
    assert_eq!(metadata["client_session_file"], "/diagnostic/session.jsonl");
    assert_eq!(metadata["client_cwd"], "/diagnostic/cwd");
}

#[tokio::test]
async fn internal_event_api_registers_agent_binding_for_any_ready_event_with_client_session_key() {
    let state = test_state().await;
    let launch_cwd = tempfile::tempdir().expect("workspace");
    let launch_cwd = launch_cwd
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let mut created = event_body(
        "evt_internal_event_generic_ready_created",
        "session.created",
        "sess_internal_event_generic_ready",
        None,
        1,
    );
    created["source"] = json!("external_api");
    created["client_type"] = json!("generic");
    post_event(state.clone(), created).await;
    sqlx::query(
        "INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, launch_cwd, metadata) VALUES (?, 'tmux', 'rtinst_generic', ?, ?)",
    )
    .bind("sess_internal_event_generic_ready")
    .bind(launch_cwd.display().to_string())
    .bind(json!({"runtime_instance_id":"rtinst_generic", "workspace": launch_cwd.display().to_string()}).to_string())
    .execute(&state.db())
    .await
    .expect("runtime binding");

    let mut event = event_body(
        "evt_internal_event_generic_ready",
        "session.ready",
        "sess_internal_event_generic_ready",
        None,
        2,
    );
    event["source"] = json!("agent_client");
    event["client_type"] = json!("generic");
    event["payload"] = json!({
        "runtime_instance_id":"rtinst_generic",
        "client_session_key":"generic_session_123"
    });

    let (status, body) = post_event(state.clone(), event).await;

    assert_eq!(status, StatusCode::OK, "{body:?}");
    let client_session_key: String = sqlx::query_scalar(
        "SELECT client_session_key FROM agent_bindings WHERE session_id = ? AND client_type = ?",
    )
    .bind("sess_internal_event_generic_ready")
    .bind("generic")
    .fetch_one(&state.db())
    .await
    .expect("agent binding row");
    assert_eq!(client_session_key, "generic_session_123");
}

#[tokio::test]
async fn internal_event_api_rejects_bound_confirmed_runtime_event_without_runtime_instance_id() {
    let state = test_state().await;
    let launch_cwd = tempfile::tempdir().expect("workspace");
    let launch_cwd = launch_cwd
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let mut created = event_body(
        "evt_internal_event_bound_missing_rt_created",
        "session.created",
        "sess_internal_event_bound_missing_rt",
        None,
        1,
    );
    created["source"] = json!("external_api");
    created["client_type"] = json!("generic");
    post_event(state.clone(), created).await;
    sqlx::query(
        "INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, launch_cwd, metadata) VALUES (?, 'generic', 'rtinst_expected', ?, ?)",
    )
    .bind("sess_internal_event_bound_missing_rt")
    .bind(launch_cwd.display().to_string())
    .bind(json!({"runtime_instance_id":"rtinst_expected", "workspace": launch_cwd.display().to_string()}).to_string())
    .execute(&state.db())
    .await
    .expect("runtime binding");

    let mut event = event_body(
        "evt_internal_event_bound_missing_rt_started",
        "turn.started",
        "sess_internal_event_bound_missing_rt",
        Some("turn_internal_event_bound_missing_rt"),
        2,
    );
    event["source"] = json!("agent_client");
    event["client_type"] = json!("generic");

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
async fn internal_event_api_rejects_confirmed_runtime_event_with_mismatched_runtime_instance_id() {
    let state = test_state().await;
    let launch_cwd = tempfile::tempdir().expect("workspace");
    let launch_cwd = launch_cwd
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let mut created = event_body(
        "evt_internal_event_bound_wrong_rt_created",
        "session.created",
        "sess_internal_event_bound_wrong_rt",
        None,
        1,
    );
    created["source"] = json!("external_api");
    created["client_type"] = json!("generic");
    post_event(state.clone(), created).await;
    sqlx::query(
        "INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, launch_cwd, metadata) VALUES (?, 'generic', 'rtinst_expected', ?, ?)",
    )
    .bind("sess_internal_event_bound_wrong_rt")
    .bind(launch_cwd.display().to_string())
    .bind(json!({"runtime_instance_id":"rtinst_expected", "workspace": launch_cwd.display().to_string()}).to_string())
    .execute(&state.db())
    .await
    .expect("runtime binding");

    let mut event = event_body(
        "evt_internal_event_bound_wrong_rt_started",
        "turn.started",
        "sess_internal_event_bound_wrong_rt",
        Some("turn_internal_event_bound_wrong_rt"),
        2,
    );
    event["source"] = json!("agent_client");
    event["client_type"] = json!("generic");
    event["payload"] = json!({"runtime_instance_id":"rtinst_wrong"});

    let (status, body) = post_event(state, event).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("does not match")
    );
}

#[tokio::test]
async fn internal_event_api_rejects_confirmed_runtime_event_with_mismatched_client_type() {
    let state = test_state().await;
    let launch_cwd = tempfile::tempdir().expect("workspace");
    let launch_cwd = launch_cwd
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let mut created = event_body(
        "evt_internal_event_bound_wrong_client_created",
        "session.created",
        "sess_internal_event_bound_wrong_client",
        None,
        1,
    );
    created["source"] = json!("external_api");
    created["client_type"] = json!("generic");
    post_event(state.clone(), created).await;
    sqlx::query(
        "INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, launch_cwd, metadata) VALUES (?, 'generic', 'rtinst_expected_client', ?, ?)",
    )
    .bind("sess_internal_event_bound_wrong_client")
    .bind(launch_cwd.display().to_string())
    .bind(json!({"runtime_instance_id":"rtinst_expected_client", "workspace": launch_cwd.display().to_string()}).to_string())
    .execute(&state.db())
    .await
    .expect("runtime binding");

    let mut event = event_body(
        "evt_internal_event_bound_wrong_client_started",
        "turn.started",
        "sess_internal_event_bound_wrong_client",
        Some("turn_internal_event_bound_wrong_client"),
        2,
    );
    event["source"] = json!("agent_client");
    event["client_type"] = json!("pi");
    event["payload"] = json!({"runtime_instance_id":"rtinst_expected_client"});

    let (status, body) = post_event(state, event).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("client_type")
    );
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
        "evt_internal_event_ready_retry_created",
        "session.created",
        "sess_internal_event_ready_retry",
        None,
        1,
    );
    created["source"] = json!("external_api");
    created["client_type"] = json!("pi");
    post_event(state.clone(), created).await;
    sqlx::query(
        "INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, launch_cwd, metadata) VALUES (?, 'tmux', 'rtinst_retry', ?, ?)",
    )
    .bind("sess_internal_event_ready_retry")
    .bind(launch_cwd.display().to_string())
    .bind(json!({"runtime_instance_id":"rtinst_retry", "workspace": launch_cwd.display().to_string()}).to_string())
    .execute(&state.db())
    .await
    .expect("runtime binding");

    for event_id in [
        "evt_internal_event_ready_retry_1",
        "evt_internal_event_ready_retry_2",
    ] {
        let mut event = event_body(
            event_id,
            "session.ready",
            "sess_internal_event_ready_retry",
            None,
            2,
        );
        event["source"] = json!("agent_client");
        event["client_type"] = json!("pi");
        event["payload"] =
            json!({"runtime_instance_id":"rtinst_retry", "client_session_key":"pi_retry"});
        let (status, body) = post_event(state.clone(), event).await;
        assert_eq!(status, StatusCode::OK, "{body:?}");
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_bindings WHERE session_id = ?")
        .bind("sess_internal_event_ready_retry")
        .fetch_one(&state.db())
        .await
        .expect("agent binding count");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn internal_event_api_rejects_confirmed_turn_event_for_unknown_session() {
    let state = test_state().await;
    let mut event = event_body(
        "evt_internal_event_unknown_turn_started",
        "turn.started",
        "sess_internal_event_unknown_turn",
        Some("turn_internal_event_unknown"),
        1,
    );
    event["source"] = json!("agent_adapter");
    event["payload"] = json!({"runtime_instance_id":"rtinst_unknown"});

    let (status, body) = post_event(state.clone(), event).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unknown session")
    );
    let service = EventIngestService::new(state.db());
    assert!(
        service
            .get_session("sess_internal_event_unknown_turn")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn internal_event_api_rejects_agent_client_ready_for_unknown_session() {
    let state = test_state().await;
    let mut event = event_body(
        "evt_internal_event_unknown_ready",
        "session.ready",
        "sess_internal_event_unknown_ready",
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
    let service = EventIngestService::new(state.db());
    assert!(
        service
            .get_session("sess_internal_event_unknown_ready")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn internal_event_api_rejects_agent_client_ready_without_runtime_instance_id() {
    let state = test_state().await;
    let mut event = event_body(
        "evt_internal_event_bad_ready",
        "session.ready",
        "sess_internal_event_bad",
        None,
        1,
    );
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
        "evt_internal_event_bad_ready_key_created",
        "session.created",
        "sess_internal_event_bad_key",
        None,
        1,
    );
    created["source"] = json!("external_api");
    created["client_type"] = json!("pi");
    post_event(state.clone(), created).await;

    let mut event = event_body(
        "evt_internal_event_bad_ready_key",
        "session.ready",
        "sess_internal_event_bad_key",
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
    let event = event_body(
        "evt_internal_event_same",
        "session.created",
        "sess_internal_event_5",
        None,
        1,
    );

    let first = post_event(state.clone(), event.clone()).await;
    let second = post_event(state.clone(), event).await;

    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(second.0, StatusCode::OK);
    assert_eq!(second.1["accepted"], true);
    assert_eq!(second.1["duplicate"], true);
    assert_eq!(second.1["state_version"], 1);

    let service = EventIngestService::new(state.db());
    assert_eq!(
        service
            .list_events("sess_internal_event_5")
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn internal_event_api_maps_domain_conflicts_to_conflict() {
    let state = test_state().await;

    post_event(
        state.clone(),
        event_body(
            "evt_internal_event_6",
            "session.created",
            "sess_internal_event_6",
            None,
            1,
        ),
    )
    .await;
    post_event(
        state.clone(),
        event_body(
            "evt_internal_event_7",
            "turn.started",
            "sess_internal_event_6",
            Some("turn_internal_event_2"),
            2,
        ),
    )
    .await;
    let (status, body) = post_event(
        state,
        event_body(
            "evt_internal_event_8",
            "turn.started",
            "sess_internal_event_6",
            Some("turn_internal_event_3"),
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
    let mut event = event_body(
        "evt_internal_event_9",
        "turn.output",
        "sess_internal_event_7",
        Some("turn_internal_event_4"),
        1,
    );
    event["payload"] = json!({ "content": "x".repeat(70_000) });

    let (status, body) = post_event(state, event).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn internal_event_api_truncates_turn_output_to_200_characters() {
    let state = test_state().await;
    post_event(
        state.clone(),
        event_body(
            "evt_output_session",
            "session.created",
            "sess_output_truncation",
            None,
            1,
        ),
    )
    .await;

    let mut event = event_body(
        "evt_output_truncated",
        "turn.output",
        "sess_output_truncation",
        Some("turn_output_truncation"),
        2,
    );
    event["payload"] = json!({ "output": { "summary": "界".repeat(201) } });

    let (status, _) = post_event(state.clone(), event).await;

    assert_eq!(status, StatusCode::OK);
    let payload: String =
        sqlx::query_scalar("SELECT payload FROM events WHERE event_id = 'evt_output_truncated'")
            .fetch_one(&state.db())
            .await
            .unwrap();
    let payload: Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(payload["output"]["summary"], "界".repeat(200));
}

#[tokio::test]
async fn internal_event_api_accepts_sequence_gaps_with_warnings() {
    let state = test_state().await;

    post_event(
        state.clone(),
        event_body(
            "evt_internal_event_10",
            "session.created",
            "sess_internal_event_8",
            None,
            1,
        ),
    )
    .await;
    let (status, body) = post_event(
        state,
        event_body(
            "evt_internal_event_11",
            "session.ready",
            "sess_internal_event_8",
            None,
            3,
        ),
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
