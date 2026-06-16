use std::{fs, path::Path};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia::{
    adapters::{ArtifactRegistration, GenericTestAdapter},
    application::{AppState, ArtifactRegistrationService},
    storage::sqlite::{connect_sqlite, run_migrations},
    transport::http,
};
use serde_json::{Value, json};
use tower::ServiceExt;

#[path = "support/generic_client.rs"]
mod generic_client;

use generic_client::GenericClientTestScope;

const TOKEN: &str = "test-token";

async fn test_state(name: &str) -> AppState {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join(format!("{name}.db"));
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    AppState::builder(db)
        .external_api_token(Some(TOKEN.to_string()))
        .build()
}

async fn request_json(
    state: AppState,
    method: &str,
    uri: &str,
    token: Option<&str>,
    idempotency_key: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    if let Some(key) = idempotency_key {
        builder = builder.header("Idempotency-Key", key);
    }

    let body = if let Some(body) = body {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
        Body::from(body.to_string())
    } else {
        Body::empty()
    };

    let response = http::router(state)
        .oneshot(builder.body(body).expect("request"))
        .await
        .expect("response");
    response_json(response).await
}

async fn get_bytes(state: AppState, uri: &str) -> (StatusCode, Vec<u8>) {
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
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes()
        .to_vec();
    (status, body)
}

async fn response_json(response: axum::response::Response) -> (StatusCode, Value) {
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

async fn create_session_with_key(state: AppState, key: &str) -> (StatusCode, Value) {
    request_json(
        state,
        "POST",
        "/external/v1/sessions",
        Some(TOKEN),
        Some(key),
        Some(json!({"client_type":"generic","workspace":"/tmp/pontia-mvp"})),
    )
    .await
}

async fn submit_turn_with_key(
    state: AppState,
    session_id: &str,
    key: &str,
    input: &str,
) -> (StatusCode, Value) {
    let (status, body) = request_json(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        Some(TOKEN),
        Some(key),
        Some(json!({"input": input, "metadata": {"scenario":"mvp"}})),
    )
    .await;
    let Some(turn_id) = body["data"]["inbox_message"]["turn_id"].as_str() else {
        return (status, body);
    };
    let (turn_status, turn_body) = request_json(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}"),
        Some(TOKEN),
        None,
        None,
    )
    .await;
    assert_eq!(turn_status, StatusCode::OK);
    (
        status,
        json!({ "data": { "turn": turn_body["data"]["turn"].clone() } }),
    )
}

async fn post_internal_event(
    state: AppState,
    event_id: &str,
    event_type: &str,
    session_id: &str,
    turn_id: Option<&str>,
    seq: i64,
    payload: Value,
) -> (StatusCode, Value) {
    request_json(
        state,
        "POST",
        "/internal/v1/events",
        None,
        None,
        Some(json!({
            "event_id": event_id,
            "session_id": session_id,
            "turn_id": turn_id,
            "source": "agent_adapter",
            "client_type": "generic",
            "type": event_type,
            "time": "2026-04-27T12:00:00Z",
            "seq": seq,
            "payload": payload,
        })),
    )
    .await
}

fn file_url(path: &Path) -> String {
    format!("file://{}", path.display())
}

#[tokio::test]
async fn orchestrator_can_complete_backend_only_http_polling_flow() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state("mvp_e2e").await;

    let (create_status, create_body) = create_session_with_key(state.clone(), "mvp-session").await;
    assert_eq!(create_status, StatusCode::CREATED);
    let session_id = create_body["data"]["session"]["session_id"]
        .as_str()
        .expect("session id")
        .to_string();
    assert_eq!(create_body["data"]["session"]["state"], "idle");
    assert_eq!(
        create_body["data"]["session"]["capabilities"]["accept_task"],
        true
    );

    let (turn_status, turn_body) =
        submit_turn_with_key(state.clone(), &session_id, "mvp-turn", "produce artifact").await;
    assert_eq!(turn_status, StatusCode::CREATED);
    let turn_id = turn_body["data"]["turn"]["turn_id"]
        .as_str()
        .expect("turn id")
        .to_string();
    assert!(turn_id.starts_with("turn_"));
    assert_eq!(turn_body["data"]["turn"]["state"], "queued");

    let accepted_inputs = GenericTestAdapter::recorded_inputs();
    assert!(accepted_inputs.iter().any(|input| {
        input.session_id == session_id
            && input.turn_id == turn_id
            && input.input == "produce artifact"
    }));

    let (interrupt_status, interrupt_body) = request_json(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/interrupt"),
        Some(TOKEN),
        Some("mvp-interrupt"),
        None,
    )
    .await;
    assert_eq!(interrupt_status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(interrupt_body["error"]["code"], "capability_unavailable");

    for (event_id, event_type, seq, payload) in [
        ("evt_mvp_started", "turn.started", 10, json!({})),
        (
            "evt_mvp_output",
            "turn.output",
            11,
            json!({"output":{"summary":"artifact ready","artifact_ids":["art_mvp_result"]}}),
        ),
        (
            "evt_mvp_completed",
            "turn.completed",
            12,
            json!({"output":{"summary":"done","artifact_ids":["art_mvp_result"]}}),
        ),
    ] {
        let (status, body) = post_internal_event(
            state.clone(),
            event_id,
            event_type,
            &session_id,
            Some(&turn_id),
            seq,
            payload,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body:?}");
    }

    let dir = tempfile::tempdir().expect("artifact dir");
    let artifact_path = dir.path().join("result.txt");
    fs::write(&artifact_path, "MVP artifact content").expect("write artifact");
    ArtifactRegistrationService::new(state.db())
        .register(ArtifactRegistration {
            artifact_id: "art_mvp_result".to_string(),
            session_id: session_id.clone(),
            turn_id: Some(turn_id.clone()),
            kind: "file".to_string(),
            name: "result.txt".to_string(),
            source_ref: file_url(&artifact_path),
            size_bytes: Some(20),
            metadata: json!({"preview":"MVP artifact content","source_ref":"must not leak"}),
        })
        .await
        .expect("register artifact");

    let (get_turn_status, get_turn_body) = request_json(
        state.clone(),
        "GET",
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}"),
        Some(TOKEN),
        None,
        None,
    )
    .await;
    assert_eq!(get_turn_status, StatusCode::OK);
    assert_eq!(get_turn_body["data"]["turn"]["state"], "completed");
    assert_eq!(
        get_turn_body["data"]["turn"]["output"]["summary"],
        "artifact ready"
    );
    assert_eq!(
        get_turn_body["data"]["turn"]["output"]["artifact_ids"][0],
        "art_mvp_result"
    );

    let (events_status, events_body) = request_json(
        state.clone(),
        "GET",
        &format!("/external/v1/sessions/{session_id}/events"),
        Some(TOKEN),
        None,
        None,
    )
    .await;
    assert_eq!(events_status, StatusCode::OK);
    let events = events_body["data"]["events"].as_array().expect("events");
    assert!(
        events
            .iter()
            .any(|event| event["type"] == "session.created")
    );
    assert!(events.iter().any(|event| event["type"] == "turn.completed"));
    assert!(
        events
            .iter()
            .all(|event| event["source"] != "runtime_backend")
    );

    let (artifacts_status, artifacts_body) = request_json(
        state.clone(),
        "GET",
        &format!("/external/v1/sessions/{session_id}/artifacts"),
        Some(TOKEN),
        None,
        None,
    )
    .await;
    assert_eq!(artifacts_status, StatusCode::OK);
    let artifact = &artifacts_body["data"]["artifacts"][0];
    assert_eq!(artifact["artifact_id"], "art_mvp_result");
    assert!(artifact.get("source_ref").is_none());
    assert!(artifact["metadata"].get("source_ref").is_none());

    let (content_status, content) = get_bytes(
        state.clone(),
        "/external/v1/artifacts/art_mvp_result/content",
    )
    .await;
    assert_eq!(content_status, StatusCode::OK);
    assert_eq!(content, b"MVP artifact content");

    let (terminate_status, terminate_body) = request_json(
        state.clone(),
        "DELETE",
        &format!("/external/v1/sessions/{session_id}"),
        Some(TOKEN),
        Some("mvp-terminate"),
        None,
    )
    .await;
    assert_eq!(terminate_status, StatusCode::OK);
    assert_eq!(terminate_body["data"]["session"]["state"], "idle");
}

#[tokio::test]
async fn external_api_has_stable_error_semantics_and_idempotency() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state("mvp_errors").await;

    let (unauth_status, unauth_body) = request_json(
        state.clone(),
        "GET",
        "/external/v1/sessions",
        None,
        None,
        None,
    )
    .await;
    assert_eq!(unauth_status, StatusCode::UNAUTHORIZED);
    assert_eq!(unauth_body["error"]["code"], "authentication_failed");

    let (invalid_status, invalid_body) = request_json(
        state.clone(),
        "POST",
        "/external/v1/sessions",
        Some(TOKEN),
        None,
        Some(json!({"client_type":"unsupported"})),
    )
    .await;
    assert_eq!(invalid_status, StatusCode::BAD_REQUEST);
    assert_eq!(invalid_body["error"]["code"], "invalid_request");

    let (not_found_status, not_found_body) = request_json(
        state.clone(),
        "GET",
        "/external/v1/sessions/sess_missing",
        Some(TOKEN),
        None,
        None,
    )
    .await;
    assert_eq!(not_found_status, StatusCode::NOT_FOUND);
    assert_eq!(not_found_body["error"]["code"], "not_found");

    let (first_create_status, first_create_body) =
        create_session_with_key(state.clone(), "mvp-idempotent-session").await;
    let (second_create_status, second_create_body) =
        create_session_with_key(state.clone(), "mvp-idempotent-session").await;
    assert_eq!(first_create_status, StatusCode::CREATED);
    assert_eq!(second_create_status, StatusCode::OK);
    let session_id = first_create_body["data"]["session"]["session_id"]
        .as_str()
        .expect("session id")
        .to_string();
    assert_eq!(
        second_create_body["data"]["session"]["session_id"],
        session_id
    );

    let (first_turn_status, first_turn_body) = submit_turn_with_key(
        state.clone(),
        &session_id,
        "mvp-idempotent-turn",
        "one turn",
    )
    .await;
    let (second_turn_status, second_turn_body) = submit_turn_with_key(
        state.clone(),
        &session_id,
        "mvp-idempotent-turn",
        "one turn",
    )
    .await;
    assert_eq!(first_turn_status, StatusCode::CREATED);
    assert_eq!(second_turn_status, StatusCode::OK);
    let turn_id = first_turn_body["data"]["turn"]["turn_id"]
        .as_str()
        .expect("turn id")
        .to_string();
    assert_eq!(second_turn_body["data"]["turn"]["turn_id"], turn_id);

    let (queued_status, queued_body) = request_json(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        Some(TOKEN),
        Some("mvp-conflicting-turn"),
        Some(json!({"input": "second active turn", "metadata": {"scenario":"mvp"}})),
    )
    .await;
    assert_eq!(queued_status, StatusCode::CREATED);
    assert_eq!(queued_body["data"]["inbox_message"]["state"], "pending");
    assert_eq!(queued_body["data"]["inbox_message"]["turn_id"], Value::Null);

    let (capability_status, capability_body) = request_json(
        state,
        "POST",
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}/interrupt"),
        Some(TOKEN),
        Some("mvp-capability"),
        None,
    )
    .await;
    assert_eq!(capability_status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(capability_body["error"]["code"], "capability_unavailable");
}
