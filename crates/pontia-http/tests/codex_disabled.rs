mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use common::test_app::TestApp;

async fn post(app: &TestApp, path: &str, body: Value) -> StatusCode {
    pontia_http::router(app.state.clone())
        .oneshot(
            Request::post(path)
                .header("authorization", "Bearer test-token")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn codex_creation_is_rejected_without_creating_a_pi_session() {
    let app = TestApp::new().await;
    assert!(app.state.clients().spec("codex").is_none());
    assert!(app.state.clients().spec("pi").is_some());
    assert_eq!(
        post(&app, "/api/v1/sessions", json!({"client_type":"codex"})).await,
        StatusCode::BAD_REQUEST
    );
    assert!(
        app.state
            .queries()
            .list_sessions(true, None, true)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn disabled_codex_sessions_remain_readable_without_control_or_exit_facts() {
    let app = TestApp::new().await;
    for state in ["idle", "starting"] {
        sqlx::query("INSERT INTO sessions(session_id,client_type,state) VALUES (?,'codex',?)")
            .bind(state)
            .bind(state)
            .execute(&app.db)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO session_runtimes(session_id, runtime_id, tmux_socket_path, tmux_pane_id, role, state, created_at) VALUES (?, ?, ?, '%404', 'tui', 'running', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))"#)
.bind(state)
.bind(format!("instance-{state}"))
.bind(app.pontia_home().path().join("missing.sock").to_str().unwrap())
            .execute(&app.db).await.unwrap();
    }
    sqlx::query("INSERT INTO events(event_id,session_id,source,client_type,event_type,occurred_at,created_at) VALUES ('starting','starting','external_api','codex','session.starting','2000-01-01T00:00:00.000Z','2000-01-01T00:00:00.000Z')")
        .execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO turns(turn_id,session_id,state) VALUES ('turn','idle','running')")
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO codex_tui_bindings(owner_session_id,target_session_id,runtime_instance_id,connected) VALUES ('idle','idle','instance-idle',TRUE)")
        .execute(&app.db).await.unwrap();

    for action in ["tui", "exit", "interrupt", "restart"] {
        assert!(
            post(&app, &format!("/api/v1/sessions/idle/{action}"), json!({}))
                .await
                .is_client_error()
        );
    }
    app.state
        .runtime_observer()
        .sweep_startup_timeouts()
        .await
        .unwrap();
    app.state
        .runtime_observer()
        .sweep_active_tmux_sessions()
        .await
        .unwrap();
    for expected in ["idle", "starting"] {
        let response = pontia_http::router(app.state.clone())
            .oneshot(
                Request::get(format!("/api/v1/sessions/{expected}"))
                    .header("authorization", "Bearer test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(body["data"]["session"]["state"], expected);
        assert_eq!(
            body["data"]["session"]["capabilities"]["accept_task"],
            false
        );
        assert_eq!(body["data"]["session"]["capabilities"]["interrupt"], false);
        assert_eq!(
            body["data"]["session"]["capabilities"]["list_models"],
            false
        );
    }
    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(events, 1);
    let turn: String = sqlx::query_scalar("SELECT state FROM turns WHERE turn_id='turn'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(turn, "running");
    let connected: bool = sqlx::query_scalar(
        "SELECT connected FROM codex_tui_bindings WHERE owner_session_id='idle'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(connected);
}
