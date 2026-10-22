use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use pontia_application::{AppState, clients::ClientRegistry};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

async fn app() -> (Router, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let db = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&db).await.unwrap();
    let mut clients = ClientRegistry::default();
    let mut pi = pontia_client_pi::registration(None);
    pi.configuration = Some(Arc::new(pontia_client_pi::config::PiConfiguration::new(
        json!({"tui_command":"file-pi"}),
        json!({"tui_command":"env-pi"}),
    )));
    clients.register(pi);
    clients.register(pontia_client_codex::registration());
    clients.register(pontia_client_acp::registration_from_sources(
        std::collections::BTreeMap::from([(
            "file-peer".into(),
            json!({"name":"File peer", "command":"agent", "args":[]}),
        )]),
    ));
    let state = AppState::builder(db, root.path().into())
        .clients(clients)
        .external_api_token(Some("test-token".into()))
        .build();
    (pontia_http::router(state), root)
}

async fn request(
    app: &Router,
    method: &str,
    path: &str,
    body: Option<Value>,
    authorized: bool,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(format!("/api/v1/agent-clients{path}"));
    if authorized {
        request = request.header("Authorization", "Bearer test-token");
    }
    let response = app
        .clone()
        .oneshot(
            request
                .header("Content-Type", "application/json")
                .body(Body::from(
                    body.map(|value| value.to_string()).unwrap_or_default(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    (status, value)
}

#[tokio::test]
async fn invalid_effective_values_remain_manageable_after_environment_changes() {
    let root = tempfile::tempdir().unwrap();
    let db = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&db).await.unwrap();
    let router = |environment| {
        let mut clients = ClientRegistry::default();
        let mut pi = pontia_client_pi::registration(None);
        pi.configuration = Some(Arc::new(pontia_client_pi::config::PiConfiguration::new(
            json!({}),
            environment,
        )));
        clients.register(pi);
        clients.register(pontia_client_codex::registration());
        pontia_http::router(
            AppState::builder(db.clone(), root.path().into())
                .clients(clients)
                .external_api_token(Some("test-token".into()))
                .build(),
        )
    };
    let with_environment = router(json!({"tui_command":"env-pi"}));
    assert_eq!(
        request(
            &with_environment,
            "PATCH",
            "/pi/default",
            Some(json!({"config_overrides":{"tui_command":null}})),
            true
        )
        .await
        .0,
        StatusCode::OK
    );
    let restarted = router(json!({}));
    let (status, listed) = request(&restarted, "GET", "", None, true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["data"]["agent_clients"].as_array().unwrap().len(), 2);
    assert_eq!(
        request(
            &restarted,
            "PATCH",
            "/codex/default",
            Some(json!({"enabled":false})),
            true
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        request(&restarted, "DELETE", "/pi/default", None, true)
            .await
            .0,
        StatusCode::OK
    );
    let (_, reset) = request(&restarted, "GET", "/pi/default", None, true).await;
    assert_eq!(
        reset["data"]["agent_client"]["effective_config"]["tui_command"],
        "pi"
    );
}

#[tokio::test]
async fn all_management_operations_require_authentication() {
    let (app, _root) = app().await;
    for (method, path, body) in [
        ("GET", "", None),
        ("GET", "/pi/default", None),
        ("PATCH", "/pi/default", Some(json!({"enabled":false}))),
        ("DELETE", "/pi/default", None),
    ] {
        let (status, envelope) = request(&app, method, path, body, false).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(envelope["data"].is_null());
    }
    let (_, result) = request(&app, "GET", "/pi/default", None, true).await;
    assert_eq!(result["data"]["agent_client"]["has_record"], false);
}

#[tokio::test]
async fn defaults_files_and_database_entries_share_stable_flat_identity() {
    let (app, _root) = app().await;
    let (_, result) = request(&app, "GET", "", None, true).await;
    let entries = result["data"]["agent_clients"].as_array().unwrap();
    let identities: Vec<_> = entries
        .iter()
        .map(|entry| {
            (
                entry["client_type"].as_str().unwrap(),
                entry["client_key"].as_str().unwrap(),
            )
        })
        .collect();
    assert!(identities.windows(2).all(|pair| pair[0] < pair[1]));
    for identity in [
        ("pi", "default"),
        ("codex", "default"),
        ("acp", "file-peer"),
        ("acp", "kimi"),
    ] {
        assert!(identities.contains(&identity));
    }
    assert!(entries.iter().all(|entry| entry["has_record"] == false
        && entry["enabled"] == false
        && entry["created_at"].is_null()
        && entry["updated_at"].is_null()));
    let (status, created) = request(
        &app,
        "PATCH",
        "/acp/pi",
        Some(json!({"config_overrides":{"name":"Custom", "command":"custom-agent", "args":[]}})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let created = &created["data"]["agent_client"];
    assert_eq!(created["short_id"], "pi");
    assert_eq!(created["enabled"], true);
    let (_, renamed) = request(
        &app,
        "PATCH",
        "/acp/pi",
        Some(json!({"config_overrides":{"name":"Renamed", "command":"custom-agent", "args":[]}})),
        true,
    )
    .await;
    let renamed = &renamed["data"]["agent_client"];
    assert_eq!(renamed["name"], "Renamed");
    assert_eq!(renamed["id"], created["id"]);
    assert_eq!(renamed["short_id"], created["short_id"]);
    let (_, pi) = request(&app, "GET", "/pi/default", None, true).await;
    assert_ne!(pi["data"]["agent_client"]["id"], created["id"]);
}

#[tokio::test]
async fn unconfigured_clients_cannot_create_sessions() {
    let (app, root) = app().await;
    for (client, metadata) in [
        ("pi", json!({})),
        ("codex", json!({})),
        ("acp", json!({"acp_agent_id":"file-peer"})),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/sessions")
                    .header("Authorization", "Bearer test-token")
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        json!({
                            "client_type":client, "workspace":root.path(), "metadata":metadata
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(body["error"]["code"], "capability_unavailable");
    }
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/sessions/overview?sections=list")
                .header("Authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert!(
        body["groups"]["list"]["sessions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn patch_preserves_omitted_management_fields_and_replaces_overrides() {
    let (app, _root) = app().await;
    let (_, original) = request(&app, "GET", "/pi/default", None, true).await;
    let (status, saved) = request(
        &app,
        "PATCH",
        "/pi/default",
        Some(json!({"enabled":false,"config_overrides":{"tui_command":"db-pi"}})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let entry = &saved["data"]["agent_client"];
    assert_eq!(entry["config_overrides"]["tui_command"], "db-pi");
    assert_eq!(entry["effective_config"]["tui_command"], "env-pi");
    assert!(entry["created_at"].is_string());
    let (status, configured) = request(
        &app,
        "PATCH",
        "/pi/default",
        Some(json!({"config_overrides":{"tui_command":"updated-pi"}})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(configured["data"]["agent_client"]["enabled"], false);
    let (_, enabled) = request(
        &app,
        "PATCH",
        "/pi/default",
        Some(json!({"enabled":true})),
        true,
    )
    .await;
    assert_eq!(
        enabled["data"]["agent_client"]["config_overrides"],
        configured["data"]["agent_client"]["config_overrides"]
    );
    let (_, replaced) = request(
        &app,
        "PATCH",
        "/pi/default",
        Some(json!({"config_overrides":{}})),
        true,
    )
    .await;
    assert_eq!(
        replaced["data"]["agent_client"]["config_overrides"],
        json!({})
    );
    assert_eq!(replaced["data"]["agent_client"]["enabled"], true);
    for _ in 0..2 {
        assert_eq!(
            request(&app, "DELETE", "/pi/default", None, true).await.0,
            StatusCode::OK
        );
    }
    let (_, reset) = request(&app, "GET", "/pi/default", None, true).await;
    assert_eq!(reset, original);
    assert_eq!(reset["data"]["agent_client"]["enabled"], false);
}

#[tokio::test]
async fn reserved_acp_ids_cannot_create_management_records_or_sessions() {
    let (app, root) = app().await;
    let (_, before) = request(&app, "GET", "", None, true).await;
    for id in [".", "..", "%2E", "%2e%2E"] {
        assert_eq!(
            request(
                &app,
                "PATCH",
                &format!("/acp/{id}"),
                Some(json!({"config_overrides":{"command":"agent"}})),
                true
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(request(&app, "GET", "", None, true).await.1, before);
    for id in [".", ".."] {
        let response = app.clone().oneshot(Request::builder().method("POST").uri("/api/v1/sessions")
            .header("Authorization", "Bearer test-token").header("Content-Type", "application/json")
            .body(Body::from(json!({"client_type":"acp", "workspace":root.path(), "metadata":{"acp_agent_id":id}}).to_string())).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn invalid_patches_are_atomic_and_unknown_queries_are_not_found() {
    let (app, _root) = app().await;
    request(
        &app,
        "PATCH",
        "/codex/default",
        Some(json!({"enabled":true})),
        true,
    )
    .await;
    let (_, original) = request(&app, "GET", "/codex/default", None, true).await;
    for invalid in [
        json!({}),
        json!({"enabled":null}),
        json!({"config_overrides":null}),
        json!({"config_overrides":[]}),
        json!({"enabled":false,"config_overrides":{"tui_command":null}}),
        json!({"enabled":false,"config_overrides":{"tui_command":42}}),
        json!({"unknown":true}),
    ] {
        assert_eq!(
            request(&app, "PATCH", "/codex/default", Some(invalid), true)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            request(&app, "GET", "/codex/default", None, true).await.1,
            original
        );
    }
    for path in ["/acp/unknown", "/pi/unknown", "/unknown/default"] {
        assert_eq!(
            request(&app, "GET", path, None, true).await.0,
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(
        request(
            &app,
            "PATCH",
            "/unknown/default",
            Some(json!({"enabled":true})),
            true
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn null_clears_inherited_launch_source_and_delete_removes_database_only_peer() {
    let (app, _root) = app().await;
    let (status, _) = request(
        &app,
        "PATCH",
        "/acp/file-peer",
        Some(json!({"config_overrides":{"registry_id":"test-agent"}})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, response) = request(
        &app,
        "PATCH",
        "/acp/file-peer",
        Some(json!({"config_overrides":{"command":null,"args":null,"registry_id":"test-agent"}})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let config = &response["data"]["agent_client"]["effective_config"];
    assert!(config["command"].is_null());
    assert_eq!(config["registry_id"], "test-agent");
    request(
        &app,
        "PATCH",
        "/acp/only-db",
        Some(json!({"config_overrides":{"command":"agent","args":[]}})),
        true,
    )
    .await;
    for _ in 0..2 {
        assert_eq!(
            request(&app, "DELETE", "/acp/only-db", None, true).await.0,
            StatusCode::OK
        );
    }
    assert_eq!(
        request(&app, "GET", "/acp/only-db", None, true).await.0,
        StatusCode::NOT_FOUND
    );
}
