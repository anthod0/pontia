#[path = "support/acp_relay.rs"]
mod acp_relay;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use pontia_agent_control::{client_contract::ClientServicePhase, clients::ClientRegistry};
use pontia_application::AppState;
use pontia_client_acp::{ACP_MAX_CAPABILITIES, AcpAgentDefinition, AcpAgents, AcpLaunch};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};
use tower::ServiceExt;

fn registration(config: AcpAgents) -> pontia_agent_control::clients::ClientRegistration {
    pontia_client_acp::registration_with_relay(config, acp_relay::executable())
}

async fn call(app: &AppState, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("Authorization", "Bearer integration-token")
        .header("Content-Type", "application/json")
        .body(if body.is_null() {
            Body::empty()
        } else {
            Body::from(body.to_string())
        })
        .unwrap();
    let response = pontia_http::router(app.clone())
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|error| panic!("{method} {path}: {status}: {error}")),
    )
}

async fn get(app: &AppState, path: &str) -> Value {
    let (status, body) = call(app, "GET", path, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["data"].clone()
}

#[tokio::test]
async fn creation_options_follow_enabled_settings_for_native_and_acp_clients() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    pontia_agent_control::workspaces::upsert_canonical_workspace(
        &pool,
        root.path().canonicalize().unwrap().to_str().unwrap(),
        None,
    )
    .await
    .unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(pontia_client_pi::registration(None));
    clients.register(pontia_client_codex::registration());
    clients.register(pontia_client_claude::registration());
    let acp = pontia_client_acp::registration();
    let service = acp.service.clone().unwrap();
    clients.register(acp);
    let app = AppState::builder(
        pontia_agent_control::AgentControl::builder(pool, root.path().into())
            .clients(clients)
            .build(),
    )
    .external_api_token(Some("integration-token".into()))
    .build();
    let task = service.start(app.control().clone()).await.unwrap().unwrap();
    let options_path = "/api/v1/session-creation-options";
    assert_eq!(get(&app, options_path).await["options"], json!([]));

    for (client, key, metadata) in [
        ("pi", "default", json!({})),
        ("codex", "default", json!({})),
        ("claude-agent", "default", json!({})),
        ("acp", "antigravity", json!({"acp_agent_id":"antigravity"})),
        ("acp", "grok", json!({"acp_agent_id":"grok"})),
        ("acp", "kiro", json!({"acp_agent_id":"kiro"})),
    ] {
        let path = format!("/api/v1/agent-clients/{client}/{key}");
        let (status, body) = call(&app, "PATCH", &path, json!({"enabled":true})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let options = get(&app, options_path).await;
        assert_eq!(options["options"].as_array().unwrap().len(), 1);
        assert_eq!(options["options"][0]["client_type"], client);
        assert_eq!(options["options"][0]["metadata"], metadata);
        assert_eq!(options["options"][0]["model_catalog_key"], json!(key));

        let bots = get(&app, "/api/v1/bots/clients").await;
        assert!(
            bots["clients"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| { item["client_type"] == client && item["client_key"] == key })
        );

        let (status, body) = call(&app, "PATCH", &path, json!({"enabled":false})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(get(&app, options_path).await["options"], json!([]));

        let (status, body) = call(&app, "PATCH", &path, json!({"enabled":true})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(get(&app, options_path).await, options);

        let (status, body) = call(&app, "DELETE", &path, Value::Null).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(get(&app, options_path).await["options"], json!([]));
    }
    app.control().shutdown().notify();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

async fn wait_turn(app: &AppState, session: &str, turn: &str, expected: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let body = get(app, &format!("/api/v1/sessions/{session}/turns/{turn}")).await;
            if body["turn"]["state"] == expected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

async fn live_stream(app: &AppState, session: &str) -> Body {
    let response = pontia_http::router(app.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/sessions/{session}/live-output/stream"))
                .header("Authorization", "Bearer integration-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.into_body()
}

async fn live_event(body: &mut Body) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let frame = body.frame().await.unwrap().unwrap();
            if let Ok(data) = frame.into_data() {
                let text = std::str::from_utf8(&data).unwrap();
                if let Some(data) = text.lines().find_map(|line| line.strip_prefix("data: ")) {
                    return serde_json::from_str(data).unwrap();
                }
            }
        }
    })
    .await
    .expect("live SSE event")
}

async fn expect_live_updates(body: &mut Body, expected: Vec<Value>) {
    let mut received = Vec::new();
    while !expected.iter().all(|update| received.contains(update)) {
        let event = live_event(body).await;
        assert_eq!(event["type"], "updates");
        received.extend(event["updates"].as_array().unwrap().iter().cloned());
    }
}

#[tokio::test]
async fn acp_tools_use_existing_http_snapshot_updates_reconnect_and_closed_events() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("live-script.json"), json!([
        [{"update":{"sessionUpdate":"tool_call","toolCallId":"native-call","title":"Execute","status":"in_progress","rawInput":{"command":"check"}}}],
        [{"update":{"sessionUpdate":"tool_call_update","toolCallId":"native-call","content":[{"type":"content","content":{"type":"text","text":"ab"}}]}},
         {"update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"body"}}}],
        [{"update":{"sessionUpdate":"tool_call_update","toolCallId":"native-call","status":"failed","rawOutput":{"exitCode":1}}}]
    ]).to_string()).unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(registration(AcpAgents::new([(
        "test".into(),
        AcpAgentDefinition {
            name: "test".into(),
            capabilities: ACP_MAX_CAPABILITIES,
            compaction: None,
            interjection: None,
            restricted_execution: None,
            new_session_instructions_meta_key: None,
            launch: AcpLaunch {
                command: "python3".into(),
                args: vec![format!(
                    "{}/../pontia-client-acp/tests/fixtures/agent.py",
                    env!("CARGO_MANIFEST_DIR")
                )],
                env: BTreeMap::from([
                    ("ACP_FIXTURE_ROOT".into(), root.path().display().to_string()),
                    ("ACP_LIVE_SCRIPT".into(), "1".into()),
                    ("ACP_NO_UPDATES".into(), "1".into()),
                ]),
            },
        },
    )])));
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    pontia_agent_control::workspaces::upsert_canonical_workspace(
        &pool,
        root.path().canonicalize().unwrap().to_str().unwrap(),
        None,
    )
    .await
    .unwrap();
    pontia_storage_sqlite::repositories::agent_clients::SqliteAgentClientRepository::new(
        pool.clone(),
    )
    .save("acp", "test", true)
    .await
    .unwrap();
    let app = AppState::builder(
        pontia_agent_control::AgentControl::builder(pool, root.path().into())
            .clients(clients.clone())
            .build(),
    )
    .external_api_token(Some("integration-token".into()))
    .build();
    let tasks = clients
        .start_services(app.control().clone(), ClientServicePhase::Transport)
        .await
        .unwrap();
    let (status, created) = call(
        &app,
        "POST",
        "/api/v1/sessions",
        json!({
            "client_type":"acp", "metadata":{"acp_agent_id":"test"}, "workspace":root.path()
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let session = created["data"]["session"]["session_id"].as_str().unwrap();
    let (_, submitted) = call(
        &app,
        "POST",
        &format!("/api/v1/sessions/{session}/inbox/messages"),
        json!({"input":"end_turn","delivery_policy":"after_idle"}),
    )
    .await;
    let turn = submitted["data"]["inbox_message"]["turn_id"]
        .as_str()
        .unwrap();
    let mut stream = live_stream(&app, session).await;
    for marker in ["update-1", "live-1-0"] {
        std::fs::write(root.path().join(marker), "").unwrap();
    }
    let first = live_event(&mut stream).await;
    assert_eq!(first["type"], "snapshot");
    assert_eq!(first["items"][0]["call_id"], "native-call");
    expect_live_updates(
        &mut stream,
        vec![json!({
            "type":"tool_status_updated","call_id":"native-call","status":{"phase":"running"}
        })],
    )
    .await;
    std::fs::write(root.path().join("live-1-1"), "").unwrap();
    expect_live_updates(&mut stream, vec![
        json!({"type":"tool_output_updated","call_id":"native-call","text":"ab","truncated":false}),
        json!({"type":"assistant_text_delta","item_id":"response","delta":"body"}),
    ]).await;
    drop(stream);
    let mut stream = live_stream(&app, session).await;
    let recovered = live_event(&mut stream).await;
    assert_eq!(recovered["type"], "snapshot");
    assert_eq!(recovered["items"].as_array().unwrap().len(), 2);
    assert_eq!(recovered["items"][0]["output"]["text"], "ab");
    assert_eq!(recovered["items"][1]["text"], "body");
    std::fs::write(root.path().join("live-1-2"), "").unwrap();
    expect_live_updates(&mut stream, vec![
        json!({"type":"tool_status_updated","call_id":"native-call","status":{"phase":"ended","outcome":"failed"}}),
        json!({"type":"tool_result","call_id":"native-call","result":{"error":{"exitCode":1},"truncated":false}}),
    ]).await;
    wait_turn(&app, session, turn, "running").await;
    std::fs::write(root.path().join("release-1"), "").unwrap();
    assert_eq!(live_event(&mut stream).await["type"], "closed");
    wait_turn(&app, session, turn, "completed").await;
    app.control().shutdown().notify();
    tokio::time::timeout(Duration::from_secs(5), tasks.join())
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn configured_agent_selection_flows_through_http_creation_input_permission_cancel_history_and_exit()
 {
    let root = tempfile::tempdir().unwrap();
    let config = AcpAgentDefinition {
        name: "Company Agent".into(),
        capabilities: ACP_MAX_CAPABILITIES,
        compaction: None,
        interjection: None,
        restricted_execution: None,
        new_session_instructions_meta_key: None,
        launch: AcpLaunch {
            command: "python3".into(),
            args: vec![format!(
                "{}/../pontia-client-acp/tests/fixtures/agent.py",
                env!("CARGO_MANIFEST_DIR")
            )],
            env: BTreeMap::from([
                ("ACP_FIXTURE_ROOT".into(), root.path().display().to_string()),
                ("ACP_PERMISSION".into(), "1".into()),
                ("ACP_MODELS".into(), "grouped".into()),
                ("ACP_SYSTEM_PROMPT".into(), "1".into()),
                ("PRIVATE_SECRET".into(), "never-expose".into()),
            ]),
        },
    };
    let mut clients = ClientRegistry::default();
    clients.register(registration(AcpAgents::new([(
        "company-id".into(),
        config,
    )])));
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    pontia_agent_control::workspaces::upsert_canonical_workspace(
        &pool,
        root.path().canonicalize().unwrap().to_str().unwrap(),
        None,
    )
    .await
    .unwrap();
    pontia_storage_sqlite::repositories::agent_clients::SqliteAgentClientRepository::new(
        pool.clone(),
    )
    .save("acp", "company-id", true)
    .await
    .unwrap();
    let app = AppState::builder(
        pontia_agent_control::AgentControl::builder(pool.clone(), root.path().into())
            .clients(clients.clone())
            .build(),
    )
    .external_api_token(Some("integration-token".into()))
    .build();
    let tasks = clients
        .start_services(app.control().clone(), ClientServicePhase::Transport)
        .await
        .unwrap();
    let options = get(&app, "/api/v1/session-creation-options").await;
    assert_eq!(options["options"].as_array().unwrap().len(), 1);
    let option = &options["options"][0];
    assert_eq!(option["client_type"], "acp");
    assert_eq!(option["metadata"], json!({"acp_agent_id":"company-id"}));
    assert!(!options.to_string().contains("never-expose"));
    let workspace_path = root.path().join("project");
    std::fs::create_dir(&workspace_path).unwrap();
    let (workspace, _) = pontia_agent_control::workspaces::upsert_canonical_workspace(
        &pool,
        workspace_path.to_str().unwrap(),
        None,
    )
    .await
    .unwrap();
    let catalog = get(
        &app,
        &format!(
            "/api/v1/agent-clients/acp/company-id/models?workspace_id={}",
            workspace.workspace_id,
        ),
    )
    .await;
    assert_eq!(catalog["default_model"], "fast");
    assert_eq!(catalog["models"][1]["id"], "deep");
    let probe_wire = std::fs::read_to_string(root.path().join("wire.jsonl")).unwrap();
    let startup: Value = serde_json::from_str(probe_wire.lines().next().unwrap()).unwrap();
    assert_eq!(startup["startup"]["cwd"], workspace_path.to_str().unwrap());
    let (status, _) = call(
        &app,
        "GET",
        "/api/v1/agent-clients/acp/company-id/models?workspace_id=missing",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    std::fs::write(root.path().join("wire.jsonl"), "").unwrap();
    let (status, result) = call(&app, "POST", "/api/v1/sessions", json!({
        "client_type":option["client_type"], "metadata":option["metadata"], "workspace_id":workspace.workspace_id, "model":"deep",
        "system_instructions":"http-instruction-marker",
        "include_standalone_session_instructions":true,
    })).await;
    assert_eq!(status, StatusCode::CREATED, "{result}");
    let session = result["data"]["session"]["session_id"].as_str().unwrap();
    let path = format!("/api/v1/sessions/{session}");
    let detail = get(&app, &path).await;
    assert_eq!(detail["session"]["model"], "deep");
    assert_eq!(detail["session"]["acp"]["agent_id"], "company-id");
    assert_eq!(detail["session"]["acp"]["agent_info"]["name"], "fixture");
    assert_eq!(detail["session"]["acp"]["system_prompt_injected"], true);
    let wire = std::fs::read_to_string(root.path().join("wire.jsonl")).unwrap();
    for request in wire
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|request| {
            matches!(
                request["method"].as_str(),
                Some("session/new" | "session/load")
            )
        })
    {
        assert_eq!(request["params"]["systemPrompt"]["mode"], "append");
        assert!(
            request["params"]["systemPrompt"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .ends_with("\n\nhttp-instruction-marker")
        );
    }
    assert_eq!(detail["session"]["capabilities"]["timeline"], true);
    assert_eq!(detail["session"]["capabilities"]["steer"], false);
    assert_eq!(detail["session"]["capabilities"]["topology"], false);
    let (status, result) = call(
        &app,
        "POST",
        &format!("{path}/inbox/messages"),
        json!({"input":"end_turn","delivery_policy":"after_idle"}),
    )
    .await;
    assert!(status.is_success(), "{result}");
    let turn = result["data"]["inbox_message"]["turn_id"].as_str().unwrap();
    std::fs::write(root.path().join("update-1"), "").unwrap();
    wait_turn(&app, session, turn, "running").await;
    let interactions = get(&app, &format!("{path}/interactions")).await;
    let interactions = interactions["interactions"].as_array().unwrap();
    assert_eq!(interactions.len(), 3);
    let id = interactions[0]["interaction_id"].as_str().unwrap();
    let (status, body) = call(
        &app,
        "PUT",
        &format!("{path}/interactions/{id}/response"),
        json!({"option_key":"always:原"}),
    )
    .await;
    assert!(status.is_success(), "{body}");
    let (status, body) = call(&app, "POST", &format!("{path}/interrupt"), json!({})).await;
    assert!(status.is_success(), "{body}");
    let turn_before_response = get(&app, &format!("{path}/turns/{turn}")).await;
    assert_eq!(turn_before_response["turn"]["state"], "running");
    std::fs::write(root.path().join("release-1"), "").unwrap();
    wait_turn(&app, session, turn, "interrupted").await;
    let history = get(&app, &format!("{path}/turns/timeline?direction=backward")).await;
    assert!(history.to_string().contains("streamed answer"), "{history}");
    let (status, body) = call(&app, "POST", &format!("{path}/exit"), json!({})).await;
    assert!(status.is_success(), "{body}");
    let detail = get(&app, &path).await;
    assert_eq!(detail["session"]["state"], "exited");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM session_runtimes WHERE session_id=?")
            .bind(session)
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    app.control().shutdown().notify();
    tokio::time::timeout(Duration::from_secs(5), tasks.join())
        .await
        .unwrap()
        .unwrap();
}
