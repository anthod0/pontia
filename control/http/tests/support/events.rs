use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use pontia_application::AppState;
use pontia_http as http;
use serde_json::Value;
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
