use super::*;
use pontia_bot::{
    helpers::RetrieveMemories,
    identity::{BotAccess, BotRole},
    memory::{Memory, MemoryCommands, MemoryRequest, MemoryService},
};

async fn retrieve(helpers: &HelperService, bot_session: &str, workspace: Option<&str>) -> String {
    let response = helpers
        .execute(HelperRequest {
            session_id: bot_session.into(),
            command: HelperCommand::Retrieve(RetrieveMemories {
                message: "Explain the release decision and conflicting evidence".into(),
                workspace_id: workspace.map(str::to_owned),
            }),
        })
        .await
        .unwrap();
    assert_eq!(response["delivery"]["status"], "accepted");
    response["session_id"].as_str().unwrap().into()
}

async fn memory(commands: &MemoryCommands, session: &str, action: &str, input: Value) -> Value {
    let request: MemoryRequest = serde_json::from_value(json!({
        "session_id": session, "command": {"action": action, "input": input}
    }))
    .unwrap();
    serde_json::to_value(commands.execute(request).await.unwrap()).unwrap()
}

#[tokio::test]
async fn retrieval_delivers_its_reply_then_exits_and_notifies_the_bot_once() {
    let (root, pool, client, control, conversation) = fixture().await;
    let (id, session) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    conversation
        .records()
        .update(
            &id,
            BotConfiguration {
                reasoning_effort: Some("low".into()),
                ..bot_configuration("Bot", MODEL)
            },
        )
        .await
        .unwrap();
    let reader = retrieve(&helpers, &session, Some("workspace-a")).await;
    assert_eq!(
        client.launched_efforts.lock().unwrap()[&reader].as_deref(),
        Some("low")
    );
    assert_eq!(
        client.session_modes.lock().unwrap()[&reader],
        pontia_core::SessionMode::Restricted
    );
    assert_eq!(
        control.queries().session_mode(&reader).await.unwrap(),
        pontia_core::SessionMode::Restricted
    );
    assert!(
        !instructions(&control, &reader)
            .await
            .contains(pontia_agent_control::standalone_session_instructions()),
        "retrieval session must not receive standalone session instructions"
    );
    assert_eq!(
        conversation.resolve(&reader).await.unwrap().role,
        BotRole::Retrieval
    );
    let created = control
        .queries()
        .get_session(&reader)
        .await
        .unwrap()
        .unwrap();
    let injected = instructions(&control, &reader).await;
    assert!(injected.contains(include_str!("../../src/helpers/retrieval.md").trim()));
    assert!(created.workspace_id.is_none());
    let shared = root.path().join("bot/readonly").canonicalize().unwrap();
    assert_eq!(created.workspace.as_deref(), shared.to_str());
    let second = retrieve(&helpers, &session, None).await;
    for session in [&reader, &second] {
        assert_launch_mode(&control, &client, session, LaunchMode::Background).await;
    }
    assert_eq!(
        client
            .launch_directories
            .lock()
            .unwrap()
            .get(&second)
            .map(String::as_str),
        shared.to_str()
    );
    let turn = control
        .queries()
        .list_turns(&reader)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert!(turn.completed_at.is_none());
    conversation
        .send(&id, "Another request while retrieval runs")
        .await
        .unwrap();
    assert_eq!(
        client.inputs.lock().unwrap().last().unwrap().session_id,
        session
    );
    let response = helpers
        .execute(HelperRequest {
            session_id: reader.clone(),
            command: HelperCommand::Reply {
                message: "Partial findings: two conflicting decisions; source unavailable".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(response["delivery"]["status"], "accepted");
    let inputs = client.inputs.lock().unwrap().clone();
    let reply = inputs.last().unwrap();
    assert_eq!(reply.session_id, session);
    assert!(
        reply
            .input
            .contains(&format!("Source: Helper\nSession: {reader}"))
    );
    assert!(
        helpers
            .list(&id)
            .await
            .unwrap()
            .iter()
            .any(|s| s.session_id == reader)
    );
    let reopened = test_conversation(control.clone());
    assert_eq!(
        reopened.resolve(&reader).await.unwrap().role,
        BotRole::Retrieval
    );
    assert!(
        reopened
            .resolve(&reader)
            .await
            .unwrap()
            .workspace_id
            .is_none()
    );
    let ordinary = helper(&helpers, &session).await;
    let _exits = report_exits(&control);
    finish(&control, &reader, EventType::TurnCompleted).await;
    wait_for_exit(&control, &reader).await;
    helpers.reconcile().await.unwrap();
    helpers.reconcile().await.unwrap();
    let notices: Vec<(String, String, bool)> = sqlx::query_as(
        "SELECT source_session_id, content, delivered FROM bot_notifications WHERE source_session_id = ?",
    )
    .bind(&reader)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].0, reader);
    assert!(notices[0].1.contains("session.exited"));
    assert!(notices[0].2);
    let inputs = client.inputs.lock().unwrap().clone();
    let reply_position = inputs
        .iter()
        .position(|input| {
            input.session_id == session
                && input
                    .input
                    .contains("Partial findings: two conflicting decisions; source unavailable")
        })
        .unwrap();
    let exit_notices: Vec<_> = inputs
        .iter()
        .enumerate()
        .filter(|(_, input)| {
            input.session_id == session
                && input.input.contains(&format!("Session: {reader}"))
                && input.input.contains("session.exited")
        })
        .collect();
    assert_eq!(exit_notices.len(), 1);
    assert!(reply_position < exit_notices[0].0);
    assert!(conversation.resolve(&reader).await.is_err());
    assert!(
        !helpers
            .list(&id)
            .await
            .unwrap()
            .iter()
            .any(|s| s.session_id == reader)
    );
    finish(&control, &ordinary, EventType::TurnCompleted).await;
    assert_eq!(
        control
            .queries()
            .get_session(&ordinary)
            .await
            .unwrap()
            .unwrap()
            .state,
        "idle"
    );
    assert!(
        helpers
            .list(&id)
            .await
            .unwrap()
            .iter()
            .any(|s| s.session_id == ordinary)
    );
    conversation.send(&id, "Use those findings").await.unwrap();
    assert_eq!(
        client.inputs.lock().unwrap().last().unwrap().session_id,
        session
    );

    finish(&control, &second, EventType::TurnFailed).await;
    helpers.reconcile().await.unwrap();
    let failure_notices: Vec<String> =
        sqlx::query_scalar("SELECT content FROM bot_notifications WHERE source_session_id = ?")
            .bind(&second)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(
        failure_notices
            .iter()
            .any(|notice| notice.contains("turn.failed"))
    );
}

#[tokio::test]
async fn retrieval_can_change_workspace_filters_and_follow_graph_without_writing_memories() {
    let (_root, pool, _client, control, conversation) = fixture().await;
    let (_id, session) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let reader = retrieve(&helpers, &session, Some("a")).await;
    let service = MemoryService::new(pool.clone());
    let commands = MemoryCommands::new(service.clone(), Arc::new(conversation.clone()));
    let mut ids = Vec::new();
    for (workspace, keyword, content) in [
        (
            Some("a"),
            "release",
            "The release is planned for Friday; investigate approval",
        ),
        (
            Some("a"),
            "approval",
            "The approval decision postpones release to Monday",
        ),
        (
            Some("b"),
            "release",
            "Another Workspace releases on Tuesday",
        ),
        (None, "release", "Device-wide release conventions"),
        (Some("a"), "release", "Archived release plan"),
    ] {
        let result = memory(&commands, &session, "create", json!({
            "kind":"decision", "content":content, "keywords":[keyword],
            "workspace_id":workspace, "session_id":"source-session", "sources":"Check the original approval discussion"
        })).await;
        ids.push(result["memory"]["id"].as_str().unwrap().to_owned());
    }
    let dreaming = BotAccess {
        role: BotRole::Dreaming,
        workspace_id: None,
    };
    for neighbor in &ids[1..] {
        service
            .execute(
                &dreaming,
                serde_json::from_value(json!({"action":"link", "input":{
                    "memory_a_id":ids[0], "memory_b_id":neighbor
                }}))
                .unwrap(),
            )
            .await
            .unwrap();
    }
    service
        .execute(
            &dreaming,
            serde_json::from_value(json!({"action":"archive", "input":{
                "id":ids[4], "archived":true
            }}))
            .unwrap(),
        )
        .await
        .unwrap();
    let before: Vec<Memory> = sqlx::query_as("SELECT * FROM bot_memories ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    let edges_before: Vec<(String, String)> = sqlx::query_as(
        "SELECT memory_a_id, memory_b_id FROM bot_memory_edges ORDER BY memory_a_id, memory_b_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let first = memory(
        &commands,
        &reader,
        "search",
        json!({"keywords":["release"], "workspace_id":"a"}),
    )
    .await;
    assert_eq!(first["results"].as_array().unwrap().len(), 1);
    assert_eq!(first["results"][0]["memory"]["id"], ids[0]);
    let rewritten = memory(
        &commands,
        &reader,
        "search",
        json!({"keywords":["approval"], "workspace_id":"a"}),
    )
    .await;
    assert_eq!(rewritten["results"][0]["memory"]["id"], ids[1]);
    let scoped = memory(
        &commands,
        &reader,
        "neighbors",
        json!({"id":ids[0], "workspace_id":"a"}),
    )
    .await;
    assert_eq!(scoped["memories"].as_array().unwrap().len(), 1);
    assert_eq!(scoped["memories"][0]["id"], ids[1]);
    let all = memory(&commands, &reader, "neighbors", json!({"id":ids[0]})).await;
    assert_eq!(all["memories"].as_array().unwrap().len(), 3);
    let other = memory(
        &commands,
        &reader,
        "search",
        json!({"keywords":["release"], "workspace_id":"b"}),
    )
    .await;
    assert_eq!(other["results"][0]["memory"]["id"], ids[2]);
    let unscoped = memory(
        &commands,
        &reader,
        "search",
        json!({"keywords":["release"]}),
    )
    .await;
    assert_eq!(unscoped["results"].as_array().unwrap().len(), 3);
    let batch = memory(&commands, &reader, "read", json!({"ids":ids})).await;
    assert_eq!(batch["memories"].as_array().unwrap().len(), 4);
    let filtered = memory(
        &commands,
        &reader,
        "read",
        json!({"ids":ids, "workspace_id":"a"}),
    )
    .await;
    assert_eq!(filtered["memories"].as_array().unwrap().len(), 2);
    let empty = memory(
        &commands,
        &reader,
        "search",
        json!({"keywords":["nonexistent"]}),
    )
    .await;
    assert_eq!(empty["results"], json!([]));
    let archived = memory(&commands, &reader, "neighbors", json!({"id":ids[4]})).await;
    assert_eq!(archived["memories"], json!([]));
    for (action, input) in [
        (
            "create",
            json!({"kind":"fact", "content":"Changed", "keywords":["release"]}),
        ),
        (
            "update",
            json!({"id":ids[0], "kind":"fact", "content":"Changed", "keywords":["release"], "importance":"normal"}),
        ),
        ("archive", json!({"id":ids[0], "archived":true})),
        ("link", json!({"memory_a_id":ids[1], "memory_b_id":ids[2]})),
        (
            "unlink",
            json!({"memory_a_id":ids[0], "memory_b_id":ids[1]}),
        ),
        ("use", json!({"ids":[ids[0]]})),
    ] {
        let result = commands
            .execute(
                serde_json::from_value(json!({
                    "session_id":reader, "command":{"action":action,"input":input}
                }))
                .unwrap(),
            )
            .await;
        assert!(
            matches!(result, Err(pontia_bot::memory::Error::Forbidden)),
            "{action}: {result:?}"
        );
    }
    let after: Vec<Memory> = sqlx::query_as("SELECT * FROM bot_memories ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(before).unwrap(),
        serde_json::to_value(after).unwrap()
    );
    let edges_after: Vec<(String, String)> = sqlx::query_as(
        "SELECT memory_a_id, memory_b_id FROM bot_memory_edges ORDER BY memory_a_id, memory_b_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(edges_before, edges_after);
    memory(&commands, &session, "use", json!({"ids":[ids[0]]})).await;
    let used = memory(&commands, &reader, "read", json!({"ids":[ids[0]]})).await;
    assert!(used["memories"][0]["last_read_at"].is_string());
}

#[tokio::test]
async fn retrieval_rejects_clients_that_cannot_run_restricted_but_ordinary_helpers_still_run() {
    let (root, _pool, client, control, conversation) = fixture().await;
    let (_, session) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation);
    *client.reject_restricted.lock().unwrap() = true;

    let result = helpers
        .execute(HelperRequest {
            session_id: session.clone(),
            command: HelperCommand::Retrieve(RetrieveMemories {
                message: "Find the release decision".into(),
                workspace_id: None,
            }),
        })
        .await;
    assert!(matches!(
        result,
        Err(pontia_bot::helpers::HelperError::Control(
            Error::CapabilityUnavailable(_)
        ))
    ));
    assert_eq!(client.session_modes.lock().unwrap().len(), 1);
    assert!(!root.path().join("bot/readonly").exists());

    let ordinary = helper(&helpers, &session).await;
    assert_eq!(
        client.session_modes.lock().unwrap()[&ordinary],
        pontia_core::SessionMode::Default
    );
    assert_eq!(
        control.queries().session_mode(&ordinary).await.unwrap(),
        pontia_core::SessionMode::Default
    );
}

#[tokio::test]
async fn retrieval_waits_for_its_native_turn_before_exiting() {
    use pontia_agent_control::client_contract::{
        AgentClientAdapter, AgentClientSpec, TurnLifecycleBehavior,
    };
    const NATIVE_TURNS: AgentClientSpec = AgentClientSpec {
        adapter: AgentClientAdapter {
            turn_lifecycle: TurnLifecycleBehavior::ClientManagedInteractive,
            ..BOT_CLIENT.adapter
        },
        ..BOT_CLIENT
    };
    let (_root, _pool, client, control, conversation) = fixture_with_client(&NATIVE_TURNS).await;
    let (_, bot_session) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation);
    let reader = retrieve(&helpers, &bot_session, None).await;
    let snapshot = control
        .queries()
        .session_snapshot(&reader)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.state, "idle");
    assert!(!snapshot.has_unsettled_input);
    assert!(GenericRuntimeManager.is_alive(&snapshot.runtimes[0].runtime_id));
    let runtime = &snapshot.runtimes[0].runtime_id;
    let ingest = control.event_ingest_service();
    let started = ingest
        .report_fact(ReportedFact {
            session_id: reader.clone(),
            turn_id: None,
            report_key: None,
            fact_type: EventType::TurnStarted,
            data: json!({"runtime_id":runtime}),
        })
        .await
        .unwrap();
    let response = helpers
        .execute(HelperRequest {
            session_id: reader.clone(),
            command: HelperCommand::Reply {
                message: "The decision was approved".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(response["delivery"]["status"], "accepted");
    assert_eq!(
        client.inputs.lock().unwrap().last().unwrap().session_id,
        bot_session
    );
    let _exits = report_exits(&control);
    report(
        &ingest,
        &reader,
        runtime,
        &started.turn_id.unwrap(),
        EventType::TurnCompleted,
    )
    .await;
    wait_for_exit(&control, &reader).await;
}
