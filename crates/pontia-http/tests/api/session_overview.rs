use crate::common::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_http as http;
use serde_json::Value;
use tower::ServiceExt;

const TOKEN: &str = "test-token";

async fn get(state: pontia_application::AppState, uri: &str) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
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
        .to_bytes();
    (status, serde_json::from_slice(&body).expect("json body"))
}

async fn fixture() -> TestApp {
    let app = TestApp::builder().in_memory_db().build().await;
    sqlx::query(
        r#"INSERT INTO workspaces (workspace_id, canonical_path, display_path, name)
           VALUES ('workspace-1', '/one', '/one', 'one'),
                  ('workspace-2', '/two', '/two', 'two'),
                  ('workspace-empty', '/empty', '/empty', 'empty')"#,
    )
    .execute(&app.db)
    .await
    .expect("insert workspaces");
    sqlx::query(
        r#"INSERT INTO sessions
           (session_id, client_type, state, workspace_id, pinned_at, archived_at,
            metadata, created_at, updated_at)
           VALUES
           ('active-z', 'generic', 'busy', 'workspace-1', NULL, NULL, '{}', '2026-01-01T00:00:00Z', '2026-01-06T00:00:00Z'),
           ('active-a', 'generic', 'ready', 'workspace-1', '2026-02-02T00:00:00Z', NULL, '{}', '2026-01-01T00:00:00Z', '2026-01-05T00:00:00Z'),
           ('pinned-exited', 'generic', 'exited', 'workspace-1', '2026-02-01T00:00:00Z', NULL, '{}', '2026-01-01T00:00:00Z', '2026-01-04T00:00:00Z'),
           ('workspace-two', 'generic', 'exited', 'workspace-2', NULL, NULL, '{}', '2026-01-01T00:00:00Z', '2026-01-03T00:00:00Z'),
           ('archived-new', 'generic', 'error', 'workspace-1', '2026-01-01T00:00:00Z', '2026-03-02T00:00:00Z', '{}', '2026-01-01T00:00:00Z', '2026-01-02T00:00:00Z'),
           ('archived-old', 'generic', 'busy', 'workspace-2', NULL, '2026-03-01T00:00:00Z', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"#,
    )
    .execute(&app.db)
    .await
    .expect("insert sessions");
    app
}

fn ids(group: &Value) -> Vec<&str> {
    group["sessions"]
        .as_array()
        .expect("sessions")
        .iter()
        .map(|session| session["session_id"].as_str().expect("session id"))
        .collect()
}

#[tokio::test]
async fn overview_returns_only_requested_overlapping_groups_with_their_defined_ordering() {
    let app = fixture().await;

    let (status, body) = get(
        app.state,
        "/api/v1/sessions/overview?sections=pinned,archived,active,list&limit=20",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.get("data").is_none());
    let groups = &body["groups"];
    assert_eq!(ids(&groups["pinned"]), ["active-a", "pinned-exited"]);
    assert_eq!(ids(&groups["archived"]), ["archived-new", "archived-old"]);
    assert_eq!(ids(&groups["active"]), ["active-z", "active-a"]);
    assert_eq!(
        ids(&groups["list"]),
        ["active-z", "active-a", "pinned-exited", "workspace-two"]
    );
    assert_eq!(groups["list"]["next_cursor"], Value::Null);
}

#[tokio::test]
async fn overview_uses_session_id_desc_to_break_equal_group_timestamps() {
    let app = fixture().await;
    sqlx::query(
        r#"UPDATE sessions
           SET pinned_at = CASE
                   WHEN pinned_at IS NOT NULL THEN '2026-02-01T00:00:00Z'
                   ELSE pinned_at
               END,
               archived_at = CASE
                   WHEN archived_at IS NOT NULL THEN '2026-03-01T00:00:00Z'
                   ELSE archived_at
               END,
               updated_at = CASE
                   WHEN archived_at IS NULL AND state NOT IN ('exited', 'error')
                       THEN '2026-04-01T00:00:00Z'
                   ELSE updated_at
               END"#,
    )
    .execute(&app.db)
    .await
    .expect("align group timestamps");

    let (status, body) = get(
        app.state,
        "/api/v1/sessions/overview?sections=pinned,archived,active",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        ids(&body["groups"]["pinned"]),
        ["pinned-exited", "active-a"]
    );
    assert_eq!(
        ids(&body["groups"]["archived"]),
        ["archived-old", "archived-new"]
    );
    assert_eq!(ids(&body["groups"]["active"]), ["active-z", "active-a"]);
}

#[tokio::test]
async fn overview_limit_does_not_truncate_non_list_groups() {
    let app = fixture().await;

    let (status, body) = get(
        app.state,
        "/api/v1/sessions/overview?sections=pinned,archived,active,list&limit=1",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body["groups"]["pinned"]).len(), 2);
    assert_eq!(ids(&body["groups"]["archived"]).len(), 2);
    assert_eq!(ids(&body["groups"]["active"]).len(), 2);
    assert_eq!(ids(&body["groups"]["list"]).len(), 1);
}

#[tokio::test]
async fn overview_omits_unrequested_groups_and_scopes_only_the_list_to_a_workspace() {
    let app = fixture().await;

    let (status, body) = get(
        app.state,
        "/api/v1/sessions/overview?sections=active,list&workspace_id=workspace-2",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let groups = &body["groups"];
    assert!(groups.get("pinned").is_none());
    assert!(groups.get("archived").is_none());
    assert_eq!(ids(&groups["active"]), ["active-z", "active-a"]);
    assert_eq!(ids(&groups["list"]), ["workspace-two"]);
}

#[tokio::test]
async fn overview_ignores_workspace_scope_when_list_is_not_requested() {
    let app = fixture().await;

    let (status, body) = get(
        app.state,
        "/api/v1/sessions/overview?sections=active&workspace_id=workspace-missing",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body["groups"]["active"]), ["active-z", "active-a"]);
}

#[tokio::test]
async fn overview_workspace_list_pages_all_unarchived_states_and_pinned_sessions() {
    let app = fixture().await;

    let (_, first) = get(
        app.state.clone(),
        "/api/v1/sessions/overview?sections=list&workspace_id=workspace-1&limit=2",
    )
    .await;
    assert_eq!(ids(&first["groups"]["list"]), ["active-z", "active-a"]);
    let cursor = first["groups"]["list"]["next_cursor"]
        .as_str()
        .expect("next cursor");

    let (status, second) = get(
        app.state,
        &format!(
            "/api/v1/sessions/overview?sections=list&workspace_id=workspace-1&limit=2&cursor={cursor}"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&second["groups"]["list"]), ["pinned-exited"]);
    assert_eq!(second["groups"]["list"]["next_cursor"], Value::Null);
}

#[tokio::test]
async fn overview_paginates_stably_without_duplicates_or_omissions() {
    let app = fixture().await;
    sqlx::query(
        r#"UPDATE sessions
           SET updated_at = '2026-04-01T00:00:00Z'
           WHERE session_id IN ('active-z', 'active-a', 'pinned-exited')"#,
    )
    .execute(&app.db)
    .await
    .expect("align timestamps");

    let (first_status, first) = get(
        app.state.clone(),
        "/api/v1/sessions/overview?sections=list&limit=2",
    )
    .await;
    assert_eq!(first_status, StatusCode::OK);
    assert_eq!(ids(&first["groups"]["list"]), ["pinned-exited", "active-z"]);
    let cursor = first["groups"]["list"]["next_cursor"]
        .as_str()
        .expect("next cursor");

    let (second_status, second) = get(
        app.state,
        &format!("/api/v1/sessions/overview?sections=list&limit=2&cursor={cursor}"),
    )
    .await;
    assert_eq!(second_status, StatusCode::OK);
    assert_eq!(
        ids(&second["groups"]["list"]),
        ["active-a", "workspace-two"]
    );
    assert_eq!(second["groups"]["list"]["next_cursor"], Value::Null);
}

#[tokio::test]
async fn overview_rejects_cursors_from_other_list_scopes() {
    let app = fixture().await;
    let (_, global_page) = get(
        app.state.clone(),
        "/api/v1/sessions/overview?sections=list&limit=1",
    )
    .await;
    let global_cursor = global_page["groups"]["list"]["next_cursor"]
        .as_str()
        .expect("global cursor");
    let (scope_status, scope_body) = get(
        app.state.clone(),
        &format!(
            "/api/v1/sessions/overview?sections=list&workspace_id=workspace-1&cursor={global_cursor}"
        ),
    )
    .await;
    assert_eq!(scope_status, StatusCode::BAD_REQUEST);
    assert_eq!(scope_body["error"]["code"], "invalid_request");

    let (_, workspace_page) = get(
        app.state.clone(),
        "/api/v1/sessions/overview?sections=list&workspace_id=workspace-1&limit=1",
    )
    .await;
    let workspace_cursor = workspace_page["groups"]["list"]["next_cursor"]
        .as_str()
        .expect("workspace cursor");
    let (other_workspace_status, other_workspace_body) = get(
        app.state,
        &format!(
            "/api/v1/sessions/overview?sections=list&workspace_id=workspace-2&cursor={workspace_cursor}"
        ),
    )
    .await;
    assert_eq!(other_workspace_status, StatusCode::BAD_REQUEST);
    assert_eq!(other_workspace_body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn overview_distinguishes_invalid_missing_and_empty_workspace_scopes() {
    let app = fixture().await;

    for uri in [
        "/api/v1/sessions/overview?sections=list&workspace_id=",
        "/api/v1/sessions/overview?sections=list&workspace_id=bad%20id",
    ] {
        let (status, body) = get(app.state.clone(), uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "invalid_request");
    }

    let (missing_status, missing_body) = get(
        app.state.clone(),
        "/api/v1/sessions/overview?sections=list&workspace_id=workspace-missing",
    )
    .await;
    assert_eq!(missing_status, StatusCode::NOT_FOUND);
    assert_eq!(missing_body["error"]["code"], "not_found");

    let (empty_status, empty_body) = get(
        app.state,
        "/api/v1/sessions/overview?sections=list&workspace_id=workspace-empty",
    )
    .await;
    assert_eq!(empty_status, StatusCode::OK);
    assert!(
        empty_body["groups"]["list"]["sessions"]
            .as_array()
            .expect("sessions")
            .is_empty()
    );
}

#[tokio::test]
async fn overview_requires_known_nonempty_sections() {
    let app = fixture().await;
    for uri in [
        "/api/v1/sessions/overview",
        "/api/v1/sessions/overview?sections=",
        "/api/v1/sessions/overview?sections=unknown",
    ] {
        let (status, body) = get(app.state.clone(), uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        assert_eq!(body["error"]["code"], "invalid_request", "{uri}");
    }
}

#[tokio::test]
async fn overview_rejects_limits_outside_the_supported_range() {
    let app = fixture().await;
    for uri in [
        "/api/v1/sessions/overview?sections=list&limit=0",
        "/api/v1/sessions/overview?sections=list&limit=201",
    ] {
        let (status, body) = get(app.state.clone(), uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        assert_eq!(body["error"]["code"], "invalid_request", "{uri}");
    }
}

#[tokio::test]
async fn overview_rejects_malformed_unsupported_or_misused_cursors() {
    let app = fixture().await;
    let unsupported_cursor = "eyJ2ZXJzaW9uIjoyLCJ3b3Jrc3BhY2VfaWQiOm51bGwsInVwZGF0ZWRfYXQiOiJ4Iiwic2Vzc2lvbl9pZCI6IngifQ";
    let malformed_timestamp_cursor = "eyJ2ZXJzaW9uIjoxLCJ3b3Jrc3BhY2VfaWQiOm51bGwsInVwZGF0ZWRfYXQiOiJ4Iiwic2Vzc2lvbl9pZCI6IngifQ";
    for uri in [
        "/api/v1/sessions/overview?sections=list&cursor=invalid",
        "/api/v1/sessions/overview?sections=active&cursor=invalid",
        &format!("/api/v1/sessions/overview?sections=list&cursor={unsupported_cursor}"),
        &format!("/api/v1/sessions/overview?sections=list&cursor={malformed_timestamp_cursor}"),
    ] {
        let (status, body) = get(app.state.clone(), uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        assert_eq!(body["error"]["code"], "invalid_request", "{uri}");
    }
}

#[tokio::test]
async fn overview_defaults_list_limit_to_fifty() {
    let app = TestApp::builder().in_memory_db().build().await;
    for index in 0..51 {
        sqlx::query(
            r#"INSERT INTO sessions
               (session_id, client_type, state, metadata, updated_at)
               VALUES (?, 'generic', 'exited', '{}', ?)"#,
        )
        .bind(format!("session-{index:02}"))
        .bind(format!("2026-01-01T00:00:{index:02}Z"))
        .execute(&app.db)
        .await
        .expect("insert session");
    }

    let (status, body) = get(app.state, "/api/v1/sessions/overview?sections=list").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["groups"]["list"]["sessions"]
            .as_array()
            .expect("sessions")
            .len(),
        50
    );
    assert!(body["groups"]["list"]["next_cursor"].is_string());
}
