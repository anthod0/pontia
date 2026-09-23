use crate::common::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_application::AppState;
use pontia_http as http;
use serde_json::{Value, json};
use tower::ServiceExt;

mod context;
mod lifecycle;
mod tmux;
mod upsert;

pub(super) async fn test_state() -> (AppState, TestApp) {
    let app = TestApp::builder()
        .database_name("runtime-binding-upsert.db")
        .external_api_token(Some("test-token".to_string()))
        .build()
        .await;
    (app.state.clone(), app)
}

pub(super) async fn post_upsert(state: AppState, body: Value) -> (StatusCode, Value) {
    registration_request(
        state,
        "runtime.register",
        json!({
            "version": pontia_runtime::pi_control::PROTOCOL_VERSION, "binding": body,
        }),
    )
    .await
}

pub(super) async fn get_session_context_by_client_session(
    state: AppState,
    client_type: &str,
    client_session_key: &str,
) -> (StatusCode, Value) {
    assert_eq!(client_type, "pi");
    let (status, value) = registration_request(
        state,
        "session.context",
        json!({"client_session_key":client_session_key}),
    )
    .await;
    if status != StatusCode::OK {
        return (status, value);
    }
    if value["session_context"].is_null() {
        return (StatusCode::NOT_FOUND, json!({}));
    }
    (status, json!({"data":value}))
}

pub(super) async fn delete_session(state: AppState, session_id: &str) -> (StatusCode, Value) {
    request_json(
        state,
        "DELETE",
        &format!("/api/v1/sessions/{session_id}"),
        None,
    )
    .await
}

pub(super) async fn request_json(
    state: AppState,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    if uri.starts_with("/api/v1/") {
        builder = builder.header(header::AUTHORIZATION, "Bearer test-token");
    }
    let response = http::router(state)
        .oneshot(
            builder
                .body(Body::from(
                    body.map(|body| body.to_string()).unwrap_or_default(),
                ))
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

pub(super) fn upsert_body(workspace: &str, pane_id: Option<&str>) -> Value {
    upsert_body_with_tmux(workspace, "/tmp/tmux-1000/default", pane_id, Some("dev"))
}

pub(super) fn upsert_body_with_tmux(
    workspace: &str,
    socket_path: &str,
    pane_id: Option<&str>,
    session_name: Option<&str>,
) -> Value {
    let tmux = pane_id.map(|pane_id| {
        json!({
            "socket_path": socket_path,
            "session_id": "$1",
            "session_name": session_name,
            "window_id": "@3",
            "window_index": 0,
            "pane_id": pane_id,
            "pane_index": 1,
            "pane_current_path": workspace
        })
    });
    json!({
        "client_type": "pi",
        "client_session_key": "pi_session_123",
        "client_session_file": "/tmp/pi/session.jsonl",
        "client_session_dir": "/tmp/pi",
        "client_cwd": workspace,
        "launch_cwd": workspace,
        "start_command": "pi --approve",
        "tmux": tmux
    })
}

// Keep the mixed HTTP/lifecycle scenarios while exercising registration over the actual RPC adapter.
async fn registration_request(state: AppState, method: &str, params: Value) -> (StatusCode, Value) {
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::UnixStream,
    };
    let (server, client) = UnixStream::pair().unwrap();
    let task = tokio::spawn(pontia_application::pi_ipc::serve_connection(state, server));
    let mut client = BufReader::new(client);
    client
        .get_mut()
        .write_all(
            format!(
                "{}\n",
                json!({"jsonrpc":"2.0","id":1,"method":method,"params":params})
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let mut line = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client.read_line(&mut line),
    )
    .await
    .unwrap()
    .unwrap();
    let response: Value = serde_json::from_str(&line).unwrap();
    drop(client);
    task.await.unwrap();
    if let Some(error) = response.get("error") {
        let (status, code) = match error["code"].as_i64().unwrap() {
            -32602 => (StatusCode::BAD_REQUEST, "invalid_request"),
            -32004 => (StatusCode::NOT_FOUND, "not_found"),
            -32009 => (StatusCode::CONFLICT, "state_conflict"),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };
        (
            status,
            json!({"error":{"code":code,"message":error["message"]}}),
        )
    } else {
        (StatusCode::OK, response["result"].clone())
    }
}
