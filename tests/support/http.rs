use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pilotfy::{application::AppState, transport::http};
use serde_json::Value;
use tower::ServiceExt;

pub const TOKEN: &str = "test-token";

pub async fn post_json(state: AppState, uri: &str, body: Value) -> (StatusCode, Value) {
    post_json_with_idempotency(state, uri, body, None).await
}

pub async fn post_json_with_idempotency(
    state: AppState,
    uri: &str,
    body: Value,
    idempotency_key: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"));
    if let Some(idempotency_key) = idempotency_key {
        builder = builder.header("Idempotency-Key", idempotency_key);
    }

    let response = http::router(state)
        .oneshot(builder.body(Body::from(body.to_string())).expect("request"))
        .await
        .expect("response");

    json_response(response).await
}

pub async fn get_json(state: AppState, uri: &str) -> (StatusCode, Value) {
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

    json_response(response).await
}

pub async fn json_response(response: axum::response::Response) -> (StatusCode, Value) {
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
