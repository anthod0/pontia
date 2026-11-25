#[path = "support/acp_relay.rs"]
mod acp_relay;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use pontia_agent_control::clients::ClientRegistry;
use pontia_application::AppState;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

async fn app() -> (Router, tempfile::TempDir) {
    app_with_acp(|_| pontia_client_acp::registration()).await
}

async fn app_with_acp(
    registration: impl FnOnce(&std::path::Path) -> pontia_agent_control::clients::ClientRegistration,
) -> (Router, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let db = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&db).await.unwrap();
    let mut clients = ClientRegistry::default();
    clients.register_integration(pontia_client_codex::setup::integration());
    clients.register_integration(pontia_client_pi::setup::integration());
    let mut pi = pontia_client_pi::registration(None);
    pi.configuration = Some(Arc::new(pontia_client_pi::config::PiConfiguration::new(
        json!({"tui_command":"file-pi"}),
        json!({"tui_command":"env-pi"}),
    )));
    clients.register(pi);
    clients.register(pontia_client_codex::registration());
    clients.register(registration(root.path()));
    let state = AppState::builder(
        pontia_agent_control::AgentControl::builder(db, root.path().into())
            .clients(clients)
            .build(),
    )
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
async fn invalid_file_values_remain_listable_and_block_enabling() {
    let root = tempfile::tempdir().unwrap();
    let db = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&db).await.unwrap();
    let mut clients = ClientRegistry::default();
    let mut pi = pontia_client_pi::registration(None);
    pi.configuration = Some(Arc::new(pontia_client_pi::config::PiConfiguration::new(
        json!({"tui_command":" "}),
        json!({}),
    )));
    clients.register(pi);
    clients.register(pontia_client_codex::registration());
    let app = pontia_http::router(
        AppState::builder(
            pontia_agent_control::AgentControl::builder(db, root.path().into())
                .clients(clients)
                .build(),
        )
        .external_api_token(Some("test-token".into()))
        .build(),
    );
    let (status, listed) = request(&app, "GET", "", None, true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["data"]["agent_clients"].as_array().unwrap().len(), 2);
    assert_eq!(
        request(
            &app,
            "PATCH",
            "/pi/default",
            Some(json!({"enabled":true})),
            true
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(
            &app,
            "PATCH",
            "/codex/default",
            Some(json!({"enabled":true})),
            true
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn all_management_operations_require_authentication() {
    let (app, _root) = app().await;
    for (method, path, body) in [
        ("GET", "", None),
        ("GET", "/pi/default", None),
        ("POST", "/pi/default/setup", None),
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
async fn management_lists_builtin_entries_and_rejects_unknown_acp_clients() {
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
    for entry in entries.iter().filter(|entry| entry["client_type"] == "acp") {
        assert_eq!(entry["setup"]["status"], "not_required");
    }
    let codex = entries
        .iter()
        .find(|entry| entry["client_type"] == "codex")
        .unwrap();
    assert_eq!(codex["setup"]["status"], "required");
    assert!(codex["setup"]["detail"].is_string());
    assert!(identities.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(
        identities
            .iter()
            .filter(|(client_type, _)| *client_type == "acp")
            .map(|(_, key)| *key)
            .collect::<Vec<_>>(),
        vec!["antigravity", "grok", "kiro"]
    );
    for identity in [("pi", "default"), ("codex", "default"), ("acp", "grok")] {
        assert!(identities.contains(&identity));
    }
    assert!(entries.iter().all(|entry| entry["has_record"] == false
        && entry["enabled"] == false
        && entry["created_at"].is_null()
        && entry["updated_at"].is_null()));
    let (status, _) = request(
        &app,
        "PATCH",
        "/acp/pi",
        Some(json!({"enabled":true})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unconfigured_clients_cannot_create_sessions() {
    let (app, root) = app().await;
    for (client, metadata) in [
        ("pi", json!({})),
        ("codex", json!({})),
        ("acp", json!({"acp_agent_id":"grok"})),
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
async fn patch_updates_the_enabled_setting_and_delete_restores_defaults() {
    let (app, _root) = app().await;
    let (_, original) = request(&app, "GET", "/pi/default", None, true).await;
    let (status, saved) = request(
        &app,
        "PATCH",
        "/pi/default",
        Some(json!({"enabled":false})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let entry = &saved["data"]["agent_client"];
    assert_eq!(entry["enabled"], false);
    assert_eq!(entry["has_record"], true);
    assert!(entry["created_at"].is_string());
    let (_, enabled) = request(
        &app,
        "PATCH",
        "/pi/default",
        Some(json!({"enabled":true})),
        true,
    )
    .await;
    assert_eq!(enabled["data"]["agent_client"]["enabled"], true);
    assert_eq!(
        enabled["data"]["agent_client"]["created_at"],
        entry["created_at"]
    );
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
                Some(json!({"enabled":true})),
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
        json!({"config_overrides":{}}),
        json!({"enabled":true,"config_overrides":{}}),
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
async fn setup_inspection_errors_are_reported_per_entry_and_rechecked_on_read() {
    use std::{fs, os::unix::fs::PermissionsExt};
    let (app, root) = app().await;
    let launcher = root.path().join("bin/codex");
    fs::create_dir(root.path().join("bin")).unwrap();
    fs::create_dir_all(root.path().join("state")).unwrap();
    fs::write(&launcher, "#!/bin/sh\n# Pontia Codex launcher\n").unwrap();
    fs::set_permissions(&launcher, fs::Permissions::from_mode(0o755)).unwrap();
    let manifest = root.path().join("state/codex-shim.json");
    fs::write(&manifest, "{").unwrap();

    let (status, response) = request(
        &app,
        "PATCH",
        "/codex/default",
        Some(json!({"enabled":false})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let entry = &response["data"]["agent_client"];
    assert_eq!(entry["setup"]["status"], "unknown");
    assert!(entry["setup"]["detail"].is_string());

    fs::remove_file(manifest).unwrap();
    let (status, response) = request(&app, "GET", "/codex/default", None, true).await;
    assert_eq!(status, StatusCode::OK);
    let current = &response["data"]["agent_client"];
    assert_eq!(current["setup"]["status"], "required");
    assert_eq!(current["detection"], entry["detection"]);
    assert_eq!(current["enabled"], false);
}

#[tokio::test]
async fn setup_is_advertised_only_for_registered_integrations_and_rejects_other_targets() {
    let (app, root) = app().await;
    let (_, pi) = request(&app, "GET", "/pi/default", None, true).await;
    assert_eq!(
        pi["data"]["agent_client"]["setup_command"],
        "pontia setup pi"
    );
    let (_, acp) = request(&app, "GET", "/acp/grok", None, true).await;
    assert_eq!(
        acp["data"]["agent_client"]["setup"]["status"],
        "not_required"
    );
    assert!(acp["data"]["agent_client"]["setup_command"].is_null());
    assert_eq!(
        request(&app, "POST", "/acp/grok/setup", None, true).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        request(&app, "POST", "/pi/unknown/setup", None, true)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert!(!root.path().join("state/client-setup").exists());
    for authorized in [false, true] {
        let mut req = Request::builder().uri("/api/v1/agent-client-setup");
        if authorized {
            req = req.header("Authorization", "Bearer test-token");
        }
        let response = app
            .clone()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if authorized {
                StatusCode::OK
            } else {
                StatusCode::UNAUTHORIZED
            }
        );
    }
}

#[tokio::test]
async fn detection_refreshes_from_files_without_changing_enabled_settings() {
    use std::{fs, os::unix::fs::PermissionsExt};
    let (app, root) = app_with_acp(|root| {
        let mut agent = pontia_client_acp::AcpAgents::builtin()
            .get("grok")
            .unwrap()
            .clone();
        agent
            .launch
            .env
            .insert("PATH".into(), root.display().to_string());
        pontia_client_acp::registration_with_relay(
            pontia_client_acp::AcpAgents::new([("grok".into(), agent)]),
            acp_relay::executable(),
        )
    })
    .await;
    let program = root.path().join("grok");
    let (status, missing) = request(
        &app,
        "PATCH",
        "/acp/grok",
        Some(json!({
            "enabled":true
        })),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        missing["data"]["agent_client"]["detection"]["status"],
        "not_detected"
    );
    fs::write(&program, "#!/bin/sh\nexit 99\n").unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let (_, found) = request(&app, "GET", "", None, true).await;
    let found = found["data"]["agent_clients"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["client_key"] == "grok")
        .unwrap();
    assert_eq!(
        missing["data"]["agent_client"]["setup"]["status"],
        "not_required"
    );
    assert_eq!(found["setup"]["status"], "not_required");
    assert_eq!(found["detection"]["status"], "detected");
    assert_eq!(found["enabled"], true);
    assert!(found["version"].is_null());
    assert!(found["version_checked_at"].is_string());
    fs::write(
        &program,
        "#!/bin/sh\n[ \"$*\" = --version ] && echo 'grok 1.0.92 (abc) [stable]'\n",
    )
    .unwrap();
    let (_, enabled) = request(
        &app,
        "PATCH",
        "/acp/grok",
        Some(json!({"enabled":true})),
        true,
    )
    .await;
    assert_eq!(enabled["data"]["agent_client"]["version"], "1.0.92");
    fs::remove_file(&program).unwrap();
    let (status, missing_again) = request(
        &app,
        "PATCH",
        "/acp/grok",
        Some(json!({"enabled": true})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(missing_again["data"]["agent_client"]["version"], "1.0.92");
    assert_ne!(
        missing_again["data"]["agent_client"]["version_checked_at"],
        enabled["data"]["agent_client"]["version_checked_at"]
    );
    assert_eq!(
        missing_again["data"]["agent_client"]["detection"]["status"],
        "not_detected"
    );
    assert_eq!(missing_again["data"]["agent_client"]["enabled"], true);
}

#[tokio::test]
async fn listing_returns_saved_details_while_the_version_check_runs() {
    use std::{fs, os::unix::fs::PermissionsExt, time::Duration};
    let root = tempfile::tempdir().unwrap();
    let program = root.path().join("pi");
    let gate = root.path().join("continue");
    fs::write(
        &program,
        format!(
            "#!/bin/sh\nwhile [ ! -e '{}' ]; do sleep 0.02; done\necho 2.3.4\n",
            gate.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();

    let db = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&db).await.unwrap();
    let repo = pontia_storage_sqlite::repositories::agent_clients::SqliteAgentClientRepository::new(
        db.clone(),
    );
    repo.save("pi", "default", true).await.unwrap();
    let mut registry = ClientRegistry::default();
    let mut pi = pontia_client_pi::registration(None);
    pi.configuration = Some(Arc::new(pontia_client_pi::config::PiConfiguration::new(
        json!({"tui_command": program}),
        json!({}),
    )));
    registry.register(pi);
    let app = pontia_http::router(
        AppState::builder(
            pontia_agent_control::AgentControl::builder(db, root.path().into())
                .clients(registry)
                .build(),
        )
        .external_api_token(Some("test-token".into()))
        .build(),
    );
    let (status, listed) = request(&app, "GET", "", None, true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(listed["data"]["agent_clients"][0]["version"].is_null());
    fs::write(gate, "").unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let (_, data) = request(&app, "GET", "/pi/default", None, true).await;
            if data["data"]["agent_client"]["version"] == "2.3.4" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("settings page triggered a background version check");
}
