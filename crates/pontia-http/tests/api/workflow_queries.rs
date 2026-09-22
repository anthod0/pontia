use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_http as http;
use pontia_storage_sqlite::repositories::workflows::{
    CreateWorkflowNodeRecord, CreateWorkflowRecord, SqliteWorkflowRepository,
};
use pontia_workflow::WorkflowQueryService;
use serde_json::Value;
use tower::ServiceExt;

use crate::common::test_app::TestApp;

fn node(id: &str, parent: Option<&str>, phase: &str, title: &str) -> CreateWorkflowNodeRecord {
    CreateWorkflowNodeRecord {
        node_id: id.to_string(),
        workflow_id: "wf_observe".to_string(),
        parent_node_id: parent.map(str::to_string),
        phase: phase.to_string(),
        title: title.to_string(),
        instructions: "Observe it".to_string(),
        inputs: "[]".to_string(),
        output: format!("{id}.md"),
        execution_profile_id: None,
        execution_profile_version: None,
    }
}

async fn get(app: &TestApp, uri: &str) -> (StatusCode, Value) {
    let response = http::router(app.state.clone())
        .oneshot(
            Request::builder()
                .uri(uri)
                .header(header::AUTHORIZATION, "Bearer test-token")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).expect("JSON response"),
    )
}

async fn create_observable_workflow(app: &TestApp) {
    SqliteWorkflowRepository::new(app.db.clone())
        .create_definition(
            CreateWorkflowRecord {
                workflow_id: "wf_observe".to_string(),
                title: "Observe workflow".to_string(),
                cwd: app.workspace().path().display().to_string(),
                state: "pending".to_string(),
            },
            vec![
                node("node_a", None, "Research", "Research one"),
                node("node_b", Some("node_a"), "Research", "Research two"),
                node("node_c", Some("node_b"), "Review", "Review"),
                node("node_d", Some("node_c"), "Research", "Research again"),
            ],
        )
        .await
        .expect("create workflow");
}

#[tokio::test]
async fn external_workflow_queries_return_ordered_nodes_for_frontend_phase_grouping() {
    let app = TestApp::new().await;
    create_observable_workflow(&app).await;

    let (list_status, list) = get(&app, "/external/v1/workflows").await;
    assert_eq!(list_status, StatusCode::OK, "{list}");
    assert_eq!(list["data"]["workflows"][0]["workflow_id"], "wf_observe");
    assert_eq!(list["data"]["workflows"][0]["current_revision"], 1);
    assert_eq!(
        list["data"]["workflows"][0]["current_phase_name"],
        "Research"
    );

    let (detail_status, detail) = get(&app, "/external/v1/workflows/wf_observe").await;
    assert_eq!(detail_status, StatusCode::OK, "{detail}");
    assert_eq!(detail["data"]["workflow"]["current_revision"], 1);
    let nodes = detail["data"]["workflow"]["nodes"]
        .as_array()
        .expect("nodes");
    assert_eq!(nodes.len(), 4);
    assert_eq!(nodes[0]["node_id"], "node_a");
    assert_eq!(nodes[1]["phase"], "Research");
    assert_eq!(nodes[2]["phase"], "Review");
    assert_eq!(nodes[3]["phase"], "Research");
    assert_eq!(nodes[0]["status"], "pending");
}

#[tokio::test]
async fn external_workflow_context_returns_current_node_instructions_and_handoffs() {
    let app = TestApp::new().await;
    let mut current = node("node_context", None, "Build", "Implement");
    current.instructions = "Implement the requested change.".to_string();
    current.inputs = r#"["requirements.md"]"#.to_string();
    current.output = "result.md".to_string();
    SqliteWorkflowRepository::new(app.db.clone())
        .create_definition(
            CreateWorkflowRecord {
                workflow_id: "wf_observe".to_string(),
                title: "Context workflow".to_string(),
                cwd: app.workspace().path().display().to_string(),
                state: "running".to_string(),
            },
            vec![current],
        )
        .await
        .expect("create workflow");
    let handoff_dir = app
        .pontia_home()
        .path()
        .join("workflows/wf_observe/handoff");
    std::fs::create_dir_all(&handoff_dir).expect("create handoff directory");
    std::fs::write(handoff_dir.join("requirements.md"), "Build it compactly.\n")
        .expect("write handoff");

    let (status, body) = get(&app, "/external/v1/workflows/wf_observe/context").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let context = &body["data"]["context"];
    assert_eq!(context["workflow"]["workflow_id"], "wf_observe");
    assert_eq!(context["workflow"]["current_revision"], 1);
    assert_eq!(
        context["current_node"]["instructions"],
        "Implement the requested change."
    );
    assert_eq!(context["current_node"]["output"], "result.md");
    assert_eq!(
        context["current_node"]["inputs"][0],
        serde_json::json!({
            "name": "requirements.md",
            "content": "Build it compactly.\n"
        })
    );
}

#[tokio::test]
async fn revision_membership_reconstructs_historical_and_current_node_chains() {
    let app = TestApp::new().await;
    create_observable_workflow(&app).await;

    sqlx::query(
        "UPDATE workflow_nodes SET retired_revision = 2 WHERE workflow_id = 'wf_observe' AND node_id IN ('node_b', 'node_c', 'node_d')",
    )
    .execute(&app.db)
    .await
    .expect("retire old suffix");
    sqlx::query(
        r#"INSERT INTO workflow_nodes
           (node_id, workflow_id, parent_node_id, node_type, phase, title, instructions, inputs,
            output, introduced_revision)
           VALUES ('node_e', 'wf_observe', 'node_a', 'agent', 'Ship', 'Ship', 'Ship it', '[]',
                   'node_e.md', 2)"#,
    )
    .execute(&app.db)
    .await
    .expect("introduce replacement suffix");
    sqlx::query("UPDATE workflows SET current_revision = 2 WHERE workflow_id = 'wf_observe'")
        .execute(&app.db)
        .await
        .expect("advance current revision");

    let queries = WorkflowQueryService::new(app.db.clone());
    let revision_one = queries
        .get_workflow_revision("wf_observe", 1)
        .await
        .expect("query revision one")
        .expect("workflow exists");
    assert_eq!(revision_one.revision, 1);
    assert!(!revision_one.current);
    assert_eq!(
        revision_one
            .nodes
            .iter()
            .map(|node| node.node_id.as_str())
            .collect::<Vec<_>>(),
        vec!["node_a", "node_b", "node_c", "node_d"]
    );
    assert_eq!(revision_one.nodes[1].introduced_revision, 1);
    assert_eq!(revision_one.nodes[1].retired_revision, Some(2));

    let revision_two = queries
        .get_workflow_revision("wf_observe", 2)
        .await
        .expect("query revision two")
        .expect("workflow exists");
    assert!(revision_two.current);
    assert_eq!(
        revision_two
            .nodes
            .iter()
            .map(|node| node.node_id.as_str())
            .collect::<Vec<_>>(),
        vec!["node_a", "node_e"]
    );

    let (revision_status, revision) =
        get(&app, "/external/v1/workflows/wf_observe/revisions/1").await;
    assert_eq!(revision_status, StatusCode::OK, "{revision}");
    assert_eq!(revision["data"]["revision"]["revision"], 1);
    assert_eq!(
        revision["data"]["revision"]["nodes"][1]["node_id"],
        "node_b"
    );

    let (status, detail) = get(&app, "/external/v1/workflows/wf_observe").await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(detail["data"]["workflow"]["current_revision"], 2);
    assert_eq!(
        detail["data"]["workflow"]["definition_file"],
        app.pontia_home()
            .path()
            .join("workflows/wf_observe/workflow.toml")
            .display()
            .to_string()
    );
    assert_eq!(detail["data"]["workflow"]["agent_total_count"], 2);
    assert_eq!(
        detail["data"]["workflow"]["nodes"]
            .as_array()
            .expect("current nodes")
            .iter()
            .map(|node| node["node_id"].as_str().expect("node id"))
            .collect::<Vec<_>>(),
        vec!["node_a", "node_e"]
    );
}

#[tokio::test]
async fn workflow_detail_and_context_expose_the_active_patch_snapshot() {
    let app = TestApp::new().await;
    create_observable_workflow(&app).await;
    sqlx::query(
        "INSERT INTO sessions (session_id, client_type, state, current_turn_id) VALUES ('sess_active_patch', 'pi', 'busy', NULL)",
    )
    .execute(&app.db)
    .await
    .expect("insert requester Session");
    sqlx::query(
        "INSERT INTO turns (turn_id, session_id, state) VALUES ('turn_active_patch', 'sess_active_patch', 'running')",
    )
    .execute(&app.db)
    .await
    .expect("insert requester Turn");
    sqlx::query("UPDATE sessions SET current_turn_id = 'turn_active_patch' WHERE session_id = 'sess_active_patch'")
        .execute(&app.db)
        .await
        .expect("set requester current Turn");
    sqlx::query(
        "UPDATE workflow_nodes SET session_id = 'sess_active_patch' WHERE node_id = 'node_a'",
    )
    .execute(&app.db)
    .await
    .expect("bind requester Node");
    sqlx::query(
        r#"INSERT INTO workflow_patches
           (patch_id, workflow_id, requesting_node_id, requesting_session_id, requesting_turn_id,
            requesting_runtime_instance_id, replanner_creation_token, base_revision, state,
            request_document_ref, request_size_bytes)
           VALUES ('patch_active', 'wf_observe', 'node_a', 'sess_active_patch',
                   'turn_active_patch', 'rt_active_patch', 'token_active_patch', 1,
                   'requested', 'patches/patch_active/request.md', 7)"#,
    )
    .execute(&app.db)
    .await
    .expect("insert active Patch");
    sqlx::query(
        "UPDATE workflows SET state = 'replanning', active_patch_id = 'patch_active' WHERE workflow_id = 'wf_observe'",
    )
    .execute(&app.db)
    .await
    .expect("activate Patch");

    let (detail_status, detail) = get(&app, "/external/v1/workflows/wf_observe").await;
    assert_eq!(detail_status, StatusCode::OK, "{detail}");
    assert_eq!(
        detail["data"]["workflow"]["active_patch"]["patch_id"],
        "patch_active"
    );
    assert!(
        detail["data"]["workflow"]["definition_file"]
            .as_str()
            .expect("absolute definition file")
            .starts_with('/')
    );

    let (context_status, context) = get(&app, "/external/v1/workflows/wf_observe/context").await;
    assert_eq!(context_status, StatusCode::OK, "{context}");
    assert_eq!(
        context["data"]["context"]["active_patch"]["patch_id"],
        "patch_active"
    );
    assert_eq!(
        context["data"]["context"]["workflow"]["current_revision"],
        1
    );
}

#[tokio::test]
async fn multiple_patch_snapshots_reconstruct_history_timeline_and_documents() {
    let app = TestApp::new().await;
    create_observable_workflow(&app).await;

    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state) VALUES
               ('sess_request_1', 'pi', 'idle'),
               ('sess_plan_1', 'pi', 'exited'),
               ('sess_request_2', 'pi', 'idle'),
               ('sess_plan_2', 'pi', 'exited'),
               ('sess_after', 'pi', 'busy')"#,
    )
    .execute(&app.db)
    .await
    .expect("insert historical sessions");
    sqlx::query(
        r#"INSERT INTO turns (turn_id, session_id, state) VALUES
               ('turn_request_1', 'sess_request_1', 'interrupted'),
               ('turn_plan_1', 'sess_plan_1', 'completed'),
               ('turn_request_2', 'sess_request_2', 'interrupted'),
               ('turn_plan_2', 'sess_plan_2', 'completed'),
               ('turn_after', 'sess_after', 'running')"#,
    )
    .execute(&app.db)
    .await
    .expect("insert historical turns");
    sqlx::query("UPDATE workflow_nodes SET session_id = 'sess_request_1' WHERE node_id = 'node_a'")
        .execute(&app.db)
        .await
        .expect("bind first requester");
    sqlx::query(
        "UPDATE workflow_nodes SET retired_revision = 2 WHERE node_id IN ('node_b', 'node_c', 'node_d')",
    )
    .execute(&app.db)
    .await
    .expect("retire revision one suffix");
    sqlx::query(
        r#"INSERT INTO workflow_nodes
           (node_id, workflow_id, parent_node_id, phase, title, instructions, inputs, output,
            introduced_revision, retired_revision, session_id)
           VALUES
           ('node_e', 'wf_observe', 'node_a', 'Build', 'Build', 'Build', '[]', 'e.md', 2, NULL, 'sess_request_2'),
           ('node_f', 'wf_observe', 'node_e', 'Test', 'Test', 'Test', '[]', 'f.md', 2, 3, NULL)"#,
    )
    .execute(&app.db)
    .await
    .expect("insert revision two");
    sqlx::query("UPDATE workflows SET current_revision = 2 WHERE workflow_id = 'wf_observe'")
        .execute(&app.db)
        .await
        .expect("advance revision two");
    sqlx::query(
        "INSERT INTO workflow_nodes (node_id, workflow_id, parent_node_id, phase, title, instructions, inputs, output, introduced_revision, session_id) VALUES ('node_g', 'wf_observe', 'node_e', 'Ship', 'Ship', 'Ship', '[]', 'g.md', 3, 'sess_after')",
    )
    .execute(&app.db)
    .await
    .expect("insert revision three");
    sqlx::query("UPDATE workflows SET current_revision = 3 WHERE workflow_id = 'wf_observe'")
        .execute(&app.db)
        .await
        .expect("advance revision three");

    sqlx::query(
        r#"INSERT INTO workflow_patches
           (patch_id, workflow_id, requesting_node_id, requesting_session_id, requesting_turn_id,
            requesting_runtime_instance_id, replanner_creation_token, replanner_session_id,
            replanner_turn_id, replanner_runtime_instance_id, base_revision, result_revision,
            state, request_document_ref, request_size_bytes, decision_document_ref,
            requested_at, planning_at, resolved_at)
           VALUES
           ('patch_1', 'wf_observe', 'node_a', 'sess_request_1', 'turn_request_1', 'rt_req_1',
            'token_1', 'sess_plan_1', 'turn_plan_1', 'rt_plan_1', 1, 2, 'applied',
            'patches/patch_1/request.md', 13, 'patches/patch_1/decision.md',
            '2026-08-14T00:00:01.000Z', '2026-08-14T00:00:03.000Z', '2026-08-14T00:00:05.000Z'),
           ('patch_2', 'wf_observe', 'node_e', 'sess_request_2', 'turn_request_2', 'rt_req_2',
            'token_2', 'sess_plan_2', 'turn_plan_2', 'rt_plan_2', 2, 3, 'applied',
            'patches/patch_2/request.md', 14, 'patches/patch_2/decision.md',
            '2026-08-14T00:00:06.000Z', '2026-08-14T00:00:08.000Z', '2026-08-14T00:00:10.000Z')"#,
    )
    .execute(&app.db)
    .await
    .expect("insert patch history");
    sqlx::query(
        r#"INSERT INTO workflow_events
           (event_id, workflow_id, sequence, event_type, payload, created_at) VALUES
           ('we_patch_1_request', 'wf_observe', 1, 'workflow.patch_requested', '{"patch_id":"patch_1","requesting_node_id":"node_a"}', '2026-08-14T00:00:01.000Z'),
           ('we_patch_1_plan', 'wf_observe', 2, 'workflow.replanner_started', '{"patch_id":"patch_1"}', '2026-08-14T00:00:03.000Z'),
           ('we_patch_1_apply', 'wf_observe', 3, 'workflow.patch_applied', '{"patch_id":"patch_1"}', '2026-08-14T00:00:05.000Z'),
           ('we_patch_2_request', 'wf_observe', 4, 'workflow.patch_requested', '{"patch_id":"patch_2","requesting_node_id":"node_e"}', '2026-08-14T00:00:06.000Z'),
           ('we_patch_2_plan', 'wf_observe', 5, 'workflow.replanner_started', '{"patch_id":"patch_2"}', '2026-08-14T00:00:08.000Z'),
           ('we_patch_2_apply', 'wf_observe', 6, 'workflow.patch_applied', '{"patch_id":"patch_2"}', '2026-08-14T00:00:10.000Z'),
           ('we_after_activate', 'wf_observe', 7, 'workflow.node_activation_requested', '{"node_id":"node_g"}', '2026-08-14T00:00:11.000Z')"#,
    )
    .execute(&app.db)
    .await
    .expect("insert workflow timeline");
    sqlx::query(
        r#"INSERT INTO events
           (event_id, session_id, turn_id, source, client_type, event_type, occurred_at, payload, created_at) VALUES
           ('ae_request_1', 'sess_request_1', 'turn_request_1', 'agent_adapter', 'pi', 'turn.interrupted', '2026-08-14T00:00:02Z', '{}', '2026-08-14T00:00:02.000Z'),
           ('ae_plan_1', 'sess_plan_1', 'turn_plan_1', 'agent_adapter', 'pi', 'turn.completed', '2026-08-14T00:00:05Z', '{}', '2026-08-14T00:00:05.000Z'),
           ('ae_request_2', 'sess_request_2', 'turn_request_2', 'agent_adapter', 'pi', 'turn.interrupted', '2026-08-14T00:00:07Z', '{}', '2026-08-14T00:00:07.000Z'),
           ('ae_plan_2', 'sess_plan_2', 'turn_plan_2', 'agent_adapter', 'pi', 'turn.completed', '2026-08-14T00:00:10Z', '{}', '2026-08-14T00:00:10.000Z'),
           ('ae_after', 'sess_after', 'turn_after', 'agent_adapter', 'pi', 'turn.started', '2026-08-14T00:00:12Z', '{}', '2026-08-14T00:00:12.000Z')"#,
    )
    .execute(&app.db)
    .await
    .expect("insert Agent lifecycle timeline");

    let document_dir = app
        .pontia_home()
        .path()
        .join("workflows/wf_observe/patches/patch_1");
    std::fs::create_dir_all(&document_dir).expect("create Patch document directory");
    std::fs::write(document_dir.join("request.md"), "change the plan").expect("write request");

    let (patch_status, patch_body) = get(&app, "/external/v1/workflows/wf_observe/patches").await;
    assert_eq!(patch_status, StatusCode::OK, "{patch_body}");
    let patches = patch_body["data"]["patches"].as_array().expect("patches");
    assert_eq!(patches.len(), 2);
    assert_eq!(patches[0]["requesting_session_id"], "sess_request_1");
    assert_eq!(patches[0]["replanner_session_id"], "sess_plan_1");
    assert_eq!(
        patches[0]["added_node_ids"],
        serde_json::json!(["node_e", "node_f"])
    );
    assert_eq!(
        patches[0]["retired_node_ids"],
        serde_json::json!(["node_b", "node_c", "node_d"])
    );
    assert_eq!(patches[1]["added_node_ids"], serde_json::json!(["node_g"]));
    assert_eq!(
        patches[1]["retired_node_ids"],
        serde_json::json!(["node_f"])
    );

    let (revision_status, revision_body) =
        get(&app, "/external/v1/workflows/wf_observe/revisions/3").await;
    assert_eq!(revision_status, StatusCode::OK, "{revision_body}");
    assert_eq!(
        revision_body["data"]["revision"]["nodes"][2]["node_id"],
        "node_g"
    );
    assert_eq!(
        revision_body["data"]["revision"]["nodes"][2]["session_id"],
        "sess_after"
    );
    assert_eq!(
        revision_body["data"]["revision"]["nodes"][2]["turn_ids"],
        serde_json::json!(["turn_after"])
    );

    let (timeline_status, timeline_body) =
        get(&app, "/external/v1/workflows/wf_observe/timeline").await;
    assert_eq!(timeline_status, StatusCode::OK, "{timeline_body}");
    let entries = timeline_body["data"]["timeline"]["entries"]
        .as_array()
        .expect("timeline entries");
    assert_eq!(
        entries.first().expect("first")["event_id"],
        "we_patch_1_request"
    );
    assert_eq!(entries.last().expect("last")["event_id"], "ae_after");
    assert_eq!(entries.last().expect("last")["node_id"], "node_g");
    assert!(entries.iter().any(|entry| {
        entry["event_id"] == "ae_plan_2"
            && entry["fact_kind"] == "agent_lifecycle"
            && entry["patch_ids"] == serde_json::json!(["patch_2"])
    }));

    let (document_status, document_body) = get(
        &app,
        "/external/v1/workflows/wf_observe/documents?ref=patches%2Fpatch_1%2Frequest.md",
    )
    .await;
    assert_eq!(document_status, StatusCode::OK, "{document_body}");
    assert_eq!(
        document_body["data"]["document"]["content"],
        "change the plan"
    );

    let (forbidden_status, forbidden_body) = get(
        &app,
        "/external/v1/workflows/wf_observe/documents?ref=..%2F..%2Fsecret",
    )
    .await;
    assert_eq!(
        forbidden_status,
        StatusCode::BAD_REQUEST,
        "{forbidden_body}"
    );
}

#[tokio::test]
async fn external_workflow_queries_reject_invalid_limits() {
    let app = TestApp::new().await;
    for value in ["0", "101", "nope", "-1"] {
        let (status, body) = get(&app, &format!("/external/v1/workflows?limit={value}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["error"]["code"], "invalid_request");
    }
}

#[tokio::test]
async fn invalid_legacy_phase_is_visible_in_list_but_detail_conflicts() {
    let app = TestApp::new().await;
    SqliteWorkflowRepository::new(app.db.clone())
        .create_definition(
            CreateWorkflowRecord {
                workflow_id: "wf_observe".to_string(),
                title: "Legacy workflow".to_string(),
                cwd: app.workspace().path().display().to_string(),
                state: "pending".to_string(),
            },
            vec![node("node_a", None, "", "Legacy")],
        )
        .await
        .expect("create legacy workflow");

    let (list_status, list) = get(&app, "/external/v1/workflows").await;
    assert_eq!(list_status, StatusCode::OK, "{list}");
    assert_eq!(
        list["data"]["workflows"][0]["observation_error"],
        "invalid_definition"
    );

    let (detail_status, detail) = get(&app, "/external/v1/workflows/wf_observe").await;
    assert_eq!(detail_status, StatusCode::CONFLICT, "{detail}");
    assert_eq!(detail["error"]["code"], "state_conflict");
}
