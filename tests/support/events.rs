use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use llmparty::{application::AppState, transport::http};
use serde_json::{Value, json};
use tower::ServiceExt;

use super::http::json_response;

pub async fn post_internal_event(state: AppState, body: Value) -> (StatusCode, Value) {
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

    json_response(response).await
}

pub fn event_body(
    event_id: &str,
    event_type: &str,
    session_id: &str,
    turn_id: &str,
    seq: i64,
) -> Value {
    json!({
        "event_id": event_id,
        "session_id": session_id,
        "turn_id": turn_id,
        "source": "agent_adapter",
        "client_type": "generic",
        "type": event_type,
        "time": "2026-05-04T00:00:00Z",
        "seq": seq,
        "payload": {}
    })
}
