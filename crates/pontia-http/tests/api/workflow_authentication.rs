use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use pontia_application::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;

use super::support::http::json_response;
use crate::common::test_app::TestApp;

const COMMAND_PATHS: [&str; 5] = [
    "/api/v1/workflows",
    "/api/v1/workflow/submissions",
    "/api/v1/workflow/patches/request",
    "/api/v1/workflow/patches/apply",
    "/api/v1/workflow/patches/block",
];

async fn post(state: AppState, path: &str, authorization: Option<&str>) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(authorization) = authorization {
        request = request.header(header::AUTHORIZATION, authorization);
    }
    let response = pontia_http::router(state)
        .oneshot(request.body(Body::from("{}")).unwrap())
        .await
        .unwrap();
    json_response(response).await
}

#[tokio::test]
async fn workflow_commands_require_authentication_before_payload_validation() {
    for token in [Some("test-token".to_owned()), None] {
        let state = TestApp::builder()
            .external_api_token(token.clone())
            .build_state()
            .await;
        for path in COMMAND_PATHS {
            for authorization in [None, Some("Bearer wrong-token"), Some("Bearer test-token")] {
                if token.is_some() && authorization == Some("Bearer test-token") {
                    continue;
                }
                let (status, body) = post(state.clone(), path, authorization).await;
                assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}: {body}");
                assert_eq!(body["error"]["code"], "authentication_failed");
                assert_eq!(body["data"], Value::Null);
                assert_eq!(body["meta"], json!({}));
            }
        }
    }
}

#[tokio::test]
async fn workflow_commands_return_the_api_envelope_for_invalid_payloads() {
    let state = TestApp::builder().build_state().await;
    for path in COMMAND_PATHS {
        let (status, body) = post(state.clone(), path, Some("Bearer test-token")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}: {body}");
        assert_eq!(body["error"]["code"], "invalid_request");
        assert_eq!(body["data"], Value::Null);
        assert_eq!(body["meta"], json!({}));
    }
}
