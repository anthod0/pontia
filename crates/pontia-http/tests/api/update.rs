use crate::common::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use pontia_application::AppState;
use serde_json::Value;
use tower::ServiceExt;

async fn blockers(state: &AppState) -> Vec<String> {
    let response = pontia_http::router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/api/v1/update/blockers")
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    body["data"]["blockers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|blocker| blocker["session_id"].as_str().unwrap().to_owned())
        .collect()
}

async fn session(state: &AppState, id: &str, status: &str) {
    sqlx::query("INSERT INTO sessions(session_id,client_type,state) VALUES (?,'generic',?)")
        .bind(id)
        .bind(status)
        .execute(&state.db())
        .await
        .unwrap();
}

#[tokio::test]
async fn update_checks_require_authentication() {
    let state = TestApp::builder().build_state().await;
    let response = pontia_http::router(state)
        .oneshot(
            Request::builder()
                .uri("/api/v1/update/blockers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn active_execution_is_reported_without_session_visibility_filters() {
    let state = TestApp::builder().build_state().await;
    for (id, status) in [
        ("starting", "starting"),
        ("busy", "busy"),
        ("approval", "waiting_for_interaction"),
        ("idle", "idle"),
    ] {
        session(&state, id, status).await;
    }
    sqlx::query("UPDATE sessions SET archived_at='2026-11-12' WHERE session_id='busy'")
        .execute(&state.db())
        .await
        .unwrap();
    for status in ["queued", "running"] {
        session(&state, status, "idle").await;
        sqlx::query("INSERT INTO turns(turn_id,session_id,state) VALUES (?,?,?)")
            .bind(status)
            .bind(status)
            .bind(status)
            .execute(&state.db())
            .await
            .unwrap();
    }
    assert_eq!(
        blockers(&state).await,
        ["approval", "busy", "queued", "running", "starting"]
    );
    sqlx::query("UPDATE turns SET state='completed'")
        .execute(&state.db())
        .await
        .unwrap();
    sqlx::query("UPDATE sessions SET state='idle'")
        .execute(&state.db())
        .await
        .unwrap();
    assert!(blockers(&state).await.is_empty());
}

#[tokio::test]
async fn interactions_block_updates_even_when_the_session_is_idle() {
    let state = TestApp::builder().build_state().await;
    for status in ["pending", "resolving", "expired"] {
        session(&state, status, "idle").await;
        sqlx::query("INSERT INTO interactions(interaction_id,session_id,type,title,options,state,requested_at) VALUES (?,?,'choice','Approval','[]',?,'2026-11-12T00:00:00Z')")
            .bind(status).bind(status).bind(status).execute(&state.db()).await.unwrap();
    }
    assert_eq!(blockers(&state).await, ["pending", "resolving"]);
    sqlx::query("UPDATE interactions SET state='resolved',response_option_key='allow',resolved_at='2026-11-12T00:01:00Z' WHERE state IN ('pending','resolving')")
        .execute(&state.db()).await.unwrap();
    assert!(blockers(&state).await.is_empty());
}

#[tokio::test]
async fn pending_and_unconfirmed_inputs_block_but_completed_history_does_not() {
    let state = TestApp::builder().build_state().await;
    for status in [
        "pending",
        "resuming",
        "dispatching",
        "dispatched",
        "unknown",
        "failed",
        "cancelled",
        "dismissed",
        "superseded",
    ] {
        session(&state, status, "idle").await;
        sqlx::query("INSERT INTO inbox_messages(message_id,session_id,state,delivery_policy,input_summary) VALUES (?,?,?,'after_idle','task')")
            .bind(status).bind(status).bind(status).execute(&state.db()).await.unwrap();
    }
    assert_eq!(
        blockers(&state).await,
        [
            "dispatched",
            "dispatching",
            "pending",
            "resuming",
            "unknown"
        ]
    );
    for id in ["dispatched", "unknown"] {
        sqlx::query("INSERT INTO turns(turn_id,session_id,state) VALUES (?,?,'completed')")
            .bind(id)
            .bind(id)
            .execute(&state.db())
            .await
            .unwrap();
        sqlx::query("UPDATE inbox_messages SET state='dispatched',turn_id=? WHERE message_id=?")
            .bind(id)
            .bind(id)
            .execute(&state.db())
            .await
            .unwrap();
    }
    assert_eq!(
        blockers(&state).await,
        ["dispatching", "pending", "resuming"]
    );
}
