use crate::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_application::{AppState, EventIngestService};
use pontia_core::domain::{EventSource, EventType, ReportedEvent, TurnTopology};
use pontia_http as http;
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "test-token";

async fn test_state() -> AppState {
    TestApp::builder()
        .database_name("external_queries.db")
        .external_api_token(Some(TOKEN.to_string()))
        .build_state()
        .await
}

fn event(
    event_id: &str,
    event_type: EventType,
    session_id: &str,
    turn_id: Option<&str>,
    payload: Value,
) -> ReportedEvent {
    ReportedEvent::new(
        event_id.to_string(),
        session_id.to_string(),
        turn_id.map(str::to_string),
        if event_type == EventType::SessionReady {
            EventSource::RuntimeManager
        } else {
            EventSource::AgentAdapter
        },
        "generic".to_string(),
        event_type,
        payload,
    )
}

async fn bind_session_to_active_workspace(state: &AppState, session_id: &str) {
    let workspace_id = format!("ws_{session_id}");
    let canonical_path = format!("/tmp/{workspace_id}");
    sqlx::query(
        r#"INSERT INTO workspaces (workspace_id, canonical_path, display_path, name, state)
           VALUES (?, ?, ?, ?, 'active')"#,
    )
    .bind(&workspace_id)
    .bind(&canonical_path)
    .bind(&canonical_path)
    .bind(&workspace_id)
    .execute(&state.db())
    .await
    .unwrap();
    sqlx::query("UPDATE sessions SET workspace_id = ?, workspace_ref = ? WHERE session_id = ?")
        .bind(&workspace_id)
        .bind(&canonical_path)
        .bind(session_id)
        .execute(&state.db())
        .await
        .unwrap();
}

async fn seed_session_turn(state: &AppState) {
    let service = EventIngestService::new(state.db());
    service
        .ingest_reported_event(event(
            "evt_external_queries_1",
            EventType::SessionCreated,
            "sess_external_queries_1",
            None,
            json!({"metadata":{"purpose":"test"}}),
        ))
        .await
        .unwrap();
    service
        .ingest_reported_event(event(
            "evt_external_queries_2",
            EventType::SessionReady,
            "sess_external_queries_1",
            None,
            json!({}),
        ))
        .await
        .unwrap();
    service
        .ingest_event_with_topology(
            event(
                "evt_external_queries_3",
                EventType::TurnStarted,
                "sess_external_queries_1",
                Some("turn_external_queries_1"),
                json!({"input":{"summary":"do work"}}),
            ),
            TurnTopology::Root,
        )
        .await
        .unwrap();
    service
        .ingest_reported_event(event(
            "evt_external_queries_4",
            EventType::TurnCompleted,
            "sess_external_queries_1",
            Some("turn_external_queries_1"),
            json!({"output":{"summary":"done"}}),
        ))
        .await
        .unwrap();
    service
        .ingest_reported_event(event(
            "evt_external_queries_5",
            EventType::TurnFailed,
            "sess_external_queries_1",
            Some("turn_external_queries_1"),
            json!({"failure":{"message":"ignored after completion"}}),
        ))
        .await
        .unwrap();
    service
        .ingest_reported_event(event(
            "evt_external_queries_0",
            EventType::TurnCompleted,
            "sess_external_queries_1",
            Some("turn_external_queries_1"),
            json!({"output":{"summary":"ignored late completion"}}),
        ))
        .await
        .unwrap();
    sqlx::query(
        "UPDATE events SET created_at = '2026-04-24T12:00:00.000Z' WHERE event_id IN ('evt_external_queries_4', 'evt_external_queries_0')",
    )
    .execute(&state.db())
    .await
    .unwrap();

    bind_session_to_active_workspace(state, "sess_external_queries_1").await;
}

async fn get(state: AppState, uri: &str, token: Option<&str>) -> (StatusCode, Value) {
    let mut builder = Request::builder().method("GET").uri(uri);
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }

    let response = http::router(state)
        .oneshot(builder.body(Body::empty()).expect("request"))
        .await
        .expect("response");

    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let json = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).expect("json body")
    };
    (status, json)
}

#[tokio::test]
async fn external_api_validates_bearer_token_explicitly() {
    let state = test_state().await;

    let valid = get(state.clone(), "/external/v1/auth/validate", Some(TOKEN)).await;
    let missing = get(state.clone(), "/external/v1/auth/validate", None).await;
    let wrong = get(state, "/external/v1/auth/validate", Some("wrong-token")).await;

    assert_eq!(valid.0, StatusCode::OK);
    assert_eq!(valid.1["error"], Value::Null);
    assert_eq!(valid.1["data"]["authenticated"], true);
    assert_eq!(missing.0, StatusCode::UNAUTHORIZED);
    assert_eq!(missing.1["data"], Value::Null);
    assert_eq!(missing.1["error"]["code"], "authentication_failed");
    assert_eq!(wrong.0, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.1["error"]["code"], "authentication_failed");
}

#[tokio::test]
async fn external_api_rejects_missing_or_wrong_bearer_token() {
    let state = test_state().await;

    let missing = get(state.clone(), "/external/v1/sessions", None).await;
    let wrong = get(state, "/external/v1/sessions", Some("wrong-token")).await;

    assert_eq!(missing.0, StatusCode::UNAUTHORIZED);
    assert_eq!(missing.1["data"], Value::Null);
    assert_eq!(missing.1["error"]["code"], "authentication_failed");
    assert_eq!(wrong.0, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.1["error"]["code"], "authentication_failed");
}

#[tokio::test]
async fn external_api_lists_and_gets_session_views() {
    let state = test_state().await;
    seed_session_turn(&state).await;

    let (list_status, list_body) = get(state.clone(), "/external/v1/sessions", Some(TOKEN)).await;
    let (get_status, get_body) = get(
        state,
        "/external/v1/sessions/sess_external_queries_1",
        Some(TOKEN),
    )
    .await;

    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(list_body["error"], Value::Null);
    assert_eq!(list_body["data"]["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(
        list_body["data"]["sessions"][0]["session_id"],
        "sess_external_queries_1"
    );
    assert_eq!(list_body["data"]["sessions"][0]["state"], "idle");
    assert_eq!(list_body["data"]["sessions"][0]["client_type"], "generic");
    assert!(list_body["data"]["sessions"][0]["capabilities"].is_object());
    assert_eq!(
        list_body["data"]["sessions"][0]["capabilities"]["context_usage"],
        "unsupported"
    );
    assert_eq!(
        list_body["data"]["sessions"][0]["context_usage"],
        Value::Null
    );

    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(
        get_body["data"]["session"]["session_id"],
        "sess_external_queries_1"
    );
    assert_eq!(
        get_body["data"]["session"]["current_turn_id"],
        "turn_external_queries_1"
    );
    assert_eq!(get_body["data"]["session"]["context_usage"], Value::Null);
}

#[tokio::test]
async fn external_api_falls_back_to_tmux_binding_capabilities_when_metadata_is_legacy() {
    let state = test_state().await;
    let service = EventIngestService::new(state.db());
    service
        .ingest_reported_event(ReportedEvent::new(
            "evt_external_queries_legacy_cap_created".to_string(),
            "sess_external_queries_legacy_cap".to_string(),
            None,
            EventSource::AgentAdapter,
            "pi".to_string(),
            EventType::SessionCreated,
            json!({}),
        ))
        .await
        .unwrap();
    service
        .ingest_reported_event(ReportedEvent::new(
            "evt_external_queries_legacy_cap_ready".to_string(),
            "sess_external_queries_legacy_cap".to_string(),
            None,
            EventSource::RuntimeManager,
            "pi".to_string(),
            EventType::SessionReady,
            json!({}),
        ))
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO runtime_bindings
           (session_id, runtime_kind, runtime_instance_id, start_command, launch_cwd, last_seen_at,
            tmux_socket_path, tmux_pane_id, metadata)
           VALUES (?, 'tmux', 'rtinst_legacy_cap', 'pi --approve', '/tmp', '2026-06-22T00:00:00Z',
                   '/tmp/tmux-1000/default', '%150', ?)"#,
    )
    .bind("sess_external_queries_legacy_cap")
    .bind(
        json!({
            "backend": "tmux",
            "tmux_socket_path": "/tmp/tmux-1000/default",
            "tmux_pane_id": "%150"
        })
        .to_string(),
    )
    .execute(&state.db())
    .await
    .unwrap();

    let (status, body) = get(
        state,
        "/external/v1/sessions/sess_external_queries_legacy_cap",
        Some(TOKEN),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["session"]["capabilities"]["accept_task"], true);
    assert_eq!(body["data"]["session"]["capabilities"]["interrupt"], true);
}

#[tokio::test]
async fn external_api_exposes_projected_session_context_usage() {
    let state = test_state().await;
    let service = EventIngestService::new(state.db());
    service
        .ingest_reported_event(event(
            "evt_external_queries_context_created",
            EventType::SessionCreated,
            "sess_external_queries_context",
            None,
            json!({"metadata":{"kept":"yes"}}),
        ))
        .await
        .unwrap();
    service
        .ingest_reported_event(event(
            "evt_external_queries_context_usage",
            EventType::SessionContextUsageUpdated,
            "sess_external_queries_context",
            None,
            json!({
                "context_usage": {
                    "used_tokens": 7,
                    "max_tokens": 10,
                    "usage_ratio": 0.7,
                    "confidence": "estimated"
                },
                "model": "m"
            }),
        ))
        .await
        .unwrap();
    bind_session_to_active_workspace(&state, "sess_external_queries_context").await;

    let (list_status, list_body) = get(state.clone(), "/external/v1/sessions", Some(TOKEN)).await;
    let (get_status, get_body) = get(
        state,
        "/external/v1/sessions/sess_external_queries_context",
        Some(TOKEN),
    )
    .await;

    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(
        list_body["data"]["sessions"][0]["context_usage"]["used_tokens"],
        7
    );
    assert_eq!(
        list_body["data"]["sessions"][0]["context_usage"]["max_tokens"],
        10
    );
    assert_eq!(
        list_body["data"]["sessions"][0]["context_usage"]["usage_ratio"],
        0.7
    );
    assert_eq!(
        list_body["data"]["sessions"][0]["context_usage"]["confidence"],
        "estimated"
    );
    assert!(list_body["data"]["sessions"][0]["context_usage"]["observed_at"].is_string());
    assert_eq!(get_body["data"]["session"]["model"], "m");
    assert!(
        get_body["data"]["session"]["context_usage"]
            .get("model")
            .is_none()
    );
    assert_eq!(get_body["data"]["session"]["metadata"]["kept"], "yes");
}

#[tokio::test]
async fn external_api_lists_and_gets_turn_views() {
    let state = test_state().await;
    seed_session_turn(&state).await;

    let (list_status, list_body) = get(
        state.clone(),
        "/external/v1/sessions/sess_external_queries_1/turns",
        Some(TOKEN),
    )
    .await;
    let (get_status, get_body) = get(
        state,
        "/external/v1/sessions/sess_external_queries_1/turns/turn_external_queries_1",
        Some(TOKEN),
    )
    .await;

    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(list_body["data"]["turns"].as_array().unwrap().len(), 1);
    assert_eq!(
        list_body["data"]["turns"][0]["turn_id"],
        "turn_external_queries_1"
    );
    assert_eq!(list_body["data"]["turns"][0]["state"], "completed");
    assert!(list_body["data"]["turns"][0].get("turn_index").is_none());
    assert_eq!(list_body["data"]["turns"][0]["topology_status"], "root");
    assert_eq!(list_body["data"]["turns"][0]["parent_turn_id"], Value::Null);
    assert!(list_body["data"]["turns"][0].get("head_cursor").is_none());
    assert!(list_body["data"]["turns"][0].get("tail_cursor").is_none());

    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(
        get_body["data"]["turn"]["turn_id"],
        "turn_external_queries_1"
    );
    assert_eq!(
        get_body["data"]["turn"]["session_id"],
        "sess_external_queries_1"
    );
    assert!(get_body["data"]["turn"].get("turn_index").is_none());
    assert_eq!(get_body["data"]["turn"]["topology_status"], "root");
    assert_eq!(get_body["data"]["turn"]["parent_turn_id"], Value::Null);
    assert!(get_body["data"]["turn"].get("head_cursor").is_none());
    assert!(get_body["data"]["turn"].get("tail_cursor").is_none());
    assert_eq!(get_body["data"]["turn"]["input"]["summary"], "do work");
    assert_eq!(get_body["data"]["turn"]["output"]["summary"], "done");
    assert_eq!(get_body["data"]["turn"]["failure"], Value::Null);
    assert!(get_body["data"]["turn"]["started_at"].is_string());
    assert!(get_body["data"]["turn"]["completed_at"].is_string());
}

#[tokio::test]
async fn external_api_lists_linked_topology_in_turn_id_order() {
    let state = test_state().await;
    let service = EventIngestService::new(state.db());
    service
        .ingest_reported_event(event(
            "evt_topology_external_session",
            EventType::SessionCreated,
            "sess_topology_external",
            None,
            json!({}),
        ))
        .await
        .unwrap();
    service
        .ingest_event_with_topology(
            event(
                "evt_topology_external_root",
                EventType::TurnStarted,
                "sess_topology_external",
                Some("turn_01900000-0000-7000-8000-000000000001"),
                json!({}),
            ),
            TurnTopology::Root,
        )
        .await
        .unwrap();
    service
        .ingest_reported_event(event(
            "evt_topology_external_root_done",
            EventType::TurnCompleted,
            "sess_topology_external",
            Some("turn_01900000-0000-7000-8000-000000000001"),
            json!({}),
        ))
        .await
        .unwrap();
    service
        .ingest_event_with_topology(
            event(
                "evt_topology_external_child",
                EventType::TurnStarted,
                "sess_topology_external",
                Some("turn_01900000-0000-7000-8000-000000000002"),
                json!({}),
            ),
            TurnTopology::linked("turn_01900000-0000-7000-8000-000000000001"),
        )
        .await
        .unwrap();
    bind_session_to_active_workspace(&state, "sess_topology_external").await;

    let (status, body) = get(
        state,
        "/external/v1/sessions/sess_topology_external/turns",
        Some(TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let turns = body["data"]["turns"].as_array().unwrap();
    assert_eq!(turns.len(), 2);
    assert_eq!(
        turns[0]["turn_id"],
        "turn_01900000-0000-7000-8000-000000000001"
    );
    assert!(turns[0].get("turn_index").is_none());
    assert_eq!(turns[0]["topology_status"], "root");
    assert_eq!(
        turns[1]["turn_id"],
        "turn_01900000-0000-7000-8000-000000000002"
    );
    assert!(turns[1].get("turn_index").is_none());
    assert_eq!(turns[1]["topology_status"], "linked");
    assert_eq!(
        turns[1]["parent_turn_id"],
        "turn_01900000-0000-7000-8000-000000000001"
    );
}

#[tokio::test]
async fn external_api_orders_turns_by_uuid_v7_id() {
    let state = test_state().await;
    EventIngestService::new(state.db())
        .ingest_reported_event(event(
            "evt_uuid_order_session",
            EventType::SessionCreated,
            "sess_uuid_order",
            None,
            json!({}),
        ))
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO turns (turn_id, session_id, state) VALUES
           ('turn_01900000-0000-7000-8000-000000000002', 'sess_uuid_order', 'completed'),
           ('turn_01900000-0000-7000-8000-000000000001', 'sess_uuid_order', 'completed')"#,
    )
    .execute(&state.db())
    .await
    .unwrap();

    let (status, body) = get(
        state,
        "/external/v1/sessions/sess_uuid_order/turns",
        Some(TOKEN),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body:?}");
    let turn_ids = body["data"]["turns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|turn| turn["turn_id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        turn_ids,
        vec![
            "turn_01900000-0000-7000-8000-000000000001",
            "turn_01900000-0000-7000-8000-000000000002"
        ]
    );
}

#[tokio::test]
async fn external_api_lists_session_and_turn_events() {
    let state = test_state().await;
    seed_session_turn(&state).await;

    let (session_status, session_body) = get(
        state.clone(),
        "/external/v1/sessions/sess_external_queries_1/events",
        Some(TOKEN),
    )
    .await;
    let (turn_status, turn_body) = get(
        state,
        "/external/v1/sessions/sess_external_queries_1/turns/turn_external_queries_1/events",
        Some(TOKEN),
    )
    .await;

    assert_eq!(session_status, StatusCode::OK);
    assert_eq!(session_body["data"]["events"].as_array().unwrap().len(), 6);
    assert_eq!(session_body["data"]["events"][0]["type"], "session.created");
    assert_eq!(session_body["data"]["events"][0]["source"], "agent_adapter");
    for event in session_body["data"]["events"].as_array().unwrap() {
        assert!(event.get("turn_topology").is_none());
        assert!(event.get("timeline_boundary").is_none());
    }

    assert_eq!(turn_status, StatusCode::OK);
    assert_eq!(turn_body["data"]["events"].as_array().unwrap().len(), 4);
    assert_eq!(
        turn_body["data"]["events"][0]["turn_id"],
        "turn_external_queries_1"
    );
}

#[tokio::test]
async fn external_api_returns_clear_not_found_errors() {
    let state = test_state().await;

    let session = get(
        state.clone(),
        "/external/v1/sessions/sess_missing",
        Some(TOKEN),
    )
    .await;

    assert_eq!(session.0, StatusCode::NOT_FOUND);
    assert_eq!(session.1["data"], Value::Null);
    assert_eq!(session.1["error"]["code"], "not_found");
}
