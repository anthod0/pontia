use super::*;
use pontia_bot::{
    dreaming::{DreamingRequest, DreamingService},
    identity::BotRole,
    memory::{MemoryCommands, MemoryService},
};

async fn latest_dreaming_session_id(control: &AgentControl) -> Option<String> {
    control
        .queries()
        .latest_owned_session("bot", "dreaming:")
        .await
        .unwrap()
        .map(|session| session.session_id)
}

#[tokio::test]
async fn one_device_round_survives_failure_and_restart_without_becoming_a_helper() {
    let (_root, pool, client, control, conversation) = fixture().await;
    let (bot_id, bot_session) = bot(&conversation).await;
    let (other_bot, _) = bot(&conversation).await;
    let memories = MemoryCommands::new(
        MemoryService::new(pool.clone()),
        Arc::new(conversation.clone()),
    );
    async fn call(
        commands: &MemoryCommands,
        session: &str,
        action: &str,
        mut input: Value,
    ) -> Value {
        let reason = input
            .as_object_mut()
            .and_then(|input| input.remove("reason"))
            .unwrap_or(json!("Test evidence"));
        serde_json::to_value(
            commands
                .execute(
                    serde_json::from_value(
                        json!({"session_id":session,"reason":reason,"command":{"action":action,"input":input}}),
                    )
                    .unwrap(),
                )
                .await
                .unwrap(),
        )
        .unwrap()
    }
    let done = call(
        &memories,
        &bot_session,
        "create",
        json!({"kind":"fact","content":"first","keywords":["first"]}),
    )
    .await["memory"]
        .clone();
    let unfinished = call(
        &memories,
        &bot_session,
        "create",
        json!({"kind":"fact","content":"second","keywords":["second"]}),
    )
    .await["memory"]
        .clone();
    let service = DreamingService::new(control.clone(), conversation.clone());
    let first = service.start().await.unwrap().unwrap();
    assert_launch_mode(&control, &client, &first, LaunchMode::Background).await;
    assert!(
        !instructions(&control, &first)
            .await
            .contains(pontia_agent_control::standalone_session_instructions()),
        "dreaming session must not receive standalone session instructions"
    );
    assert_eq!(
        conversation.resolve(&first).await.unwrap().role,
        BotRole::Dreaming
    );
    let credential = client.tokens.lock().unwrap()[&first].clone();
    assert!(
        conversation
            .authenticate_tool(&first, Some(&credential))
            .await
            .is_ok()
    );
    assert!(
        conversation
            .authenticate_tool(&first, Some("wrong"))
            .await
            .is_err()
    );
    assert!(service.start().await.unwrap().is_none());
    let helpers = pontia_bot::helpers::HelperService::new(control.clone(), conversation.clone());
    assert!(helpers.list(&bot_id).await.unwrap().is_empty());
    assert!(helpers.list(&other_bot).await.unwrap().is_empty());
    call(
        &memories,
        &first,
        "organize",
        json!({"id":done["id"],"organized":true,"reason":"Evidence checked"}),
    )
    .await;
    call(&memories, &first, "update", json!({"id":unfinished["id"],"kind":"fact","content":"partial work","keywords":["second"],"importance":"normal"})).await;
    let before = client.inputs.lock().unwrap().len();
    let unauthorized: DreamingRequest = serde_json::from_value(json!({"session_id":bot_session,"command":{"action":"clarify","input":{"bot_id":other_bot,"message":"Unauthorized clarification"}}})).unwrap();
    assert!(service.execute(unauthorized).await.is_err());
    assert_eq!(client.inputs.lock().unwrap().len(), before);
    let clarify: DreamingRequest = serde_json::from_value(json!({"session_id":first,"command":{"action":"clarify","input":{"bot_id":other_bot,"message":"Which evidence applies?"}}})).unwrap();
    service.execute(clarify).await.unwrap();
    assert_eq!(client.inputs.lock().unwrap().len(), before + 1);
    let target = conversation
        .records()
        .get(&other_bot)
        .await
        .unwrap()
        .current_session_id
        .unwrap();
    assert_eq!(
        client.inputs.lock().unwrap().last().unwrap().session_id,
        target
    );
    let _exits = report_exits(&control);
    finish(&control, &first, EventType::TurnFailed).await;
    wait_for_exit(&control, &first).await;
    let restarted = DreamingService::new(control.clone(), conversation.clone());
    let second = restarted.start().await.unwrap().unwrap();
    assert_launch_mode(&control, &client, &second, LaunchMode::Background).await;
    assert_ne!(first, second);
    assert!(conversation.resolve(&first).await.is_err());
    let remaining = call(&memories, &second, "pending", json!({})).await;
    assert_eq!(remaining["memories"].as_array().unwrap().len(), 1);
    assert_eq!(remaining["memories"][0]["id"], unfinished["id"]);
    assert_eq!(remaining["memories"][0]["content"], "partial work");
    call(
        &memories,
        &second,
        "organize",
        json!({"id":unfinished["id"],"organized":true,"reason":"Finished remaining work"}),
    )
    .await;
    finish(&control, &second, EventType::TurnCompleted).await;
    wait_for_exit(&control, &second).await;
    assert!(restarted.start().await.unwrap().is_none());
}

#[tokio::test]
async fn hourly_runner_waits_skips_overlap_and_does_not_retry_failed_rounds() {
    let (_root, pool, _client, control, conversation) = fixture().await;
    let (_, bot_session) = bot(&conversation).await;
    let memories = MemoryCommands::new(MemoryService::new(pool), Arc::new(conversation.clone()));
    memories.execute(serde_json::from_value(json!({"session_id":bot_session,"command":{"action":"create","input":{"kind":"fact","content":"pending","keywords":["pending"]}}})).unwrap()).await.unwrap();
    // SQLite workers finish outside Tokio. Prevent automatic clock jumps while awaiting them.
    let manual_clock = tokio::spawn(async {
        loop {
            tokio::task::yield_now().await;
        }
    });
    tokio::time::pause();
    let (shutdown, receiver) = tokio::sync::watch::channel(false);
    let task = tokio::spawn(DreamingService::new(control.clone(), conversation).run(receiver));
    tokio::task::yield_now().await;
    tokio::time::advance(std::time::Duration::from_secs(3599)).await;
    assert!(latest_dreaming_session_id(&control).await.is_none());
    tokio::time::advance(std::time::Duration::from_secs(1)).await;
    // Database and launch work run on real worker threads, independent of the paused clock.
    tokio::time::resume();
    let first = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(id) = latest_dreaming_session_id(&control).await
                && !control.queries().list_turns(&id).await.unwrap().is_empty()
            {
                break id;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(3600)).await;
    assert_eq!(
        latest_dreaming_session_id(&control).await.as_deref(),
        Some(first.as_str())
    );
    tokio::time::resume();
    finish(&control, &first, EventType::TurnFailed).await;
    let runtime = control
        .queries()
        .session_snapshot(&first)
        .await
        .unwrap()
        .unwrap()
        .runtimes[0]
        .runtime_id
        .clone();
    control
        .event_ingest_service()
        .report_client_fact(
            &first,
            Some(&runtime),
            EventType::SessionExited,
            json!({"reason":"test failure"}),
        )
        .await
        .unwrap();
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(3590)).await;
    assert_eq!(
        latest_dreaming_session_id(&control).await.as_deref(),
        Some(first.as_str())
    );
    tokio::time::advance(std::time::Duration::from_secs(10)).await;
    tokio::time::resume();
    let next = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(id) = latest_dreaming_session_id(&control).await
                && id != first
                && !control.queries().list_turns(&id).await.unwrap().is_empty()
            {
                break id;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_ne!(next, first);
    let pending = memories
        .execute(
            serde_json::from_value(
                json!({"session_id":next,"command":{"action":"pending","input":{}}}),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(pending).unwrap()["memories"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    finish(&control, &next, EventType::TurnInterrupted).await;
    let restart = DreamingService::new(control.clone(), test_conversation(control.clone()));
    assert!(restart.start().await.unwrap().is_none());
    let runtime = control
        .queries()
        .session_snapshot(&next)
        .await
        .unwrap()
        .unwrap()
        .runtimes[0]
        .runtime_id
        .clone();
    assert!(!GenericRuntimeManager.is_alive(&runtime));
    control
        .event_ingest_service()
        .report_client_fact(
            &next,
            Some(&runtime),
            EventType::SessionExited,
            json!({"reason":"test interrupted round exit"}),
        )
        .await
        .unwrap();
    assert!(restart.start().await.unwrap().is_some());
    shutdown.send(true).unwrap();
    task.await.unwrap();
    manual_clock.abort();
}

#[tokio::test]
async fn accepted_first_input_keeps_an_idle_round_alive_until_its_native_turn_arrives() {
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
    let (_root, pool, client, control, conversation) = fixture_with_client(&NATIVE_TURNS).await;
    let (_, bot_session) = bot(&conversation).await;
    let memories = MemoryCommands::new(MemoryService::new(pool), Arc::new(conversation.clone()));
    memories.execute(serde_json::from_value(json!({"session_id":bot_session,"command":{"action":"create","input":{"kind":"fact","content":"pending","keywords":["pending"]}}})).unwrap()).await.unwrap();
    let service = DreamingService::new(control.clone(), conversation.clone());
    let round = service.start().await.unwrap().unwrap();
    let snapshot = control
        .queries()
        .session_snapshot(&round)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.state, "idle");
    assert!(snapshot.turns.is_empty());
    assert_eq!(snapshot.inbox[0].state, "dispatched");
    let restarted = DreamingService::new(control.clone(), conversation);
    assert!(restarted.start().await.unwrap().is_none());
    assert!(GenericRuntimeManager.is_alive(&snapshot.runtimes[0].runtime_id));
    assert_eq!(
        client
            .inputs
            .lock()
            .unwrap()
            .iter()
            .filter(|input| input.session_id == round)
            .count(),
        1
    );
    let runtime = &snapshot.runtimes[0].runtime_id;
    let ingest = control.event_ingest_service();
    let started = ingest.report_fact(ReportedFact {
        session_id: round.clone(),
        turn_id: None,
        report_key: Some("delayed-start".into()),
        fact_type: EventType::TurnStarted,
        data: json!({"runtime_id":runtime,"metadata":{"inbox_message_id":snapshot.inbox[0].message_id}}),
    }).await.unwrap();
    let turn = started.turn_id.unwrap();
    report(&ingest, &round, runtime, &turn, EventType::TurnCompleted).await;
    let _exits = report_exits(&control);
    wait_for_exit(&control, &round).await;
    assert_ne!(restarted.start().await.unwrap().unwrap(), round);
}

async fn seed_pending_memory(
    pool: sqlx::SqlitePool,
    conversation: &ConversationService,
    bot_session: &str,
) {
    let memories = MemoryCommands::new(MemoryService::new(pool), Arc::new(conversation.clone()));
    memories
        .execute(
            serde_json::from_value(json!({
                "session_id": bot_session,
                "command": {
                    "action": "create",
                    "input": {
                        "kind": "fact",
                        "content": "pending memory",
                        "keywords": ["pending"]
                    }
                }
            }))
            .unwrap(),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn bot_helper_and_dreaming_enforce_yolo_on_restricted_client_policy() {
    let (_root, pool, client, control, conversation) = fixture().await;
    assert_eq!(
        client.restricted_execution_policy("default"),
        Some(pontia_core::execution_policy::ExecutionPolicy {
            tools: None,
            disabled_tools: vec!["edit".into(), "write".into()],
            disable_mcp: true,
            disable_skills: false,
            yolo: false,
        })
    );
    let (_, bot_session) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let helper_session = helper(&helpers, &bot_session).await;
    seed_pending_memory(pool, &conversation, &bot_session).await;
    let dreaming = DreamingService::new(control.clone(), conversation.clone());
    let dreaming_session = dreaming.start().await.unwrap().unwrap();

    let expected = pontia_core::execution_policy::ExecutionPolicy {
        tools: None,
        disabled_tools: vec!["edit".into(), "write".into()],
        disable_mcp: true,
        disable_skills: false,
        yolo: true,
    };
    for session in [&bot_session, &helper_session, &dreaming_session] {
        let mut expected = expected.clone();
        expected.disable_skills = session != &helper_session;
        assert_eq!(
            client.launched_policies.lock().unwrap().get(session),
            Some(&expected)
        );
        assert_eq!(
            control
                .queries()
                .session_execution_policy(session)
                .await
                .unwrap(),
            expected
        );
    }
}

#[tokio::test]
async fn dreaming_rejects_clients_without_approval_free_restricted_policy_before_creating_state() {
    let (root, pool, client, control, conversation) = fixture().await;
    let (_, bot_session) = bot(&conversation).await;
    seed_pending_memory(pool, &conversation, &bot_session).await;
    let dreaming = DreamingService::new(control.clone(), conversation.clone());
    assert!(conversation.supports_bot_client("generic", "default"));
    assert!(!conversation.supports_bot_client("generic", "other"));

    for support in [
        PolicySupport::NoRestriction,
        PolicySupport::RejectYolo,
        PolicySupport::RejectDisableSkills,
    ] {
        *client.policy_support.lock().unwrap() = support;
        assert!(!conversation.supports_bot_client("generic", "default"));
        assert!(matches!(
            dreaming.start().await,
            Err(Error::CapabilityUnavailable(_))
        ));
        assert!(latest_dreaming_session_id(&control).await.is_none());
        assert!(!root.path().join("bot").join("readonly").exists());
    }
}

#[tokio::test]
async fn dreaming_uses_the_oldest_bots_current_model_and_built_in_instructions() {
    let (_root, pool, client, control, conversation) = fixture().await;
    let first = conversation
        .records()
        .create(bot_configuration("First", "old-model"), Default::default())
        .await
        .unwrap();
    let (second_id, second_session) = bot(&conversation).await;
    seed_pending_memory(pool, &conversation, &second_session).await;
    conversation
        .records()
        .update(&second_id, bot_configuration("Second", "other-model"))
        .await
        .unwrap();
    conversation
        .records()
        .update(&first.bot_id, bot_configuration("First", MODEL))
        .await
        .unwrap();
    let dreaming = DreamingService::new(control.clone(), conversation);
    let round = dreaming.start().await.unwrap().unwrap();
    assert_eq!(
        instructions(&control, &round).await.trim(),
        include_str!("../../src/dreaming/instructions.md").trim()
    );
    let expected = pontia_core::execution_policy::ExecutionPolicy {
        tools: None,
        disabled_tools: vec!["edit".into(), "write".into()],
        disable_mcp: true,
        disable_skills: true,
        yolo: true,
    };
    assert_eq!(
        client.launched_policies.lock().unwrap().get(&round),
        Some(&expected)
    );
    assert_eq!(
        control
            .queries()
            .session_execution_policy(&round)
            .await
            .unwrap(),
        expected
    );
}

#[tokio::test]
async fn blocked_dreaming_interaction_is_rejected_and_terminated_without_wedging_next_round() {
    use pontia_agent_control::interactions::{
        ChoiceInteractionRequest, InteractionIntent, InteractionOption, InteractionState,
    };

    let (root, pool, client, control, conversation) = fixture().await;
    let (_, bot_session) = bot(&conversation).await;
    seed_pending_memory(pool, &conversation, &bot_session).await;
    let dreaming = DreamingService::new(control.clone(), conversation);

    let first = dreaming.start().await.unwrap().unwrap();
    let snapshot = control
        .queries()
        .session_snapshot(&first)
        .await
        .unwrap()
        .unwrap();
    let runtime = snapshot.runtimes[0].runtime_id.clone();
    let turn = control.queries().list_turns(&first).await.unwrap()[0]
        .turn_id
        .clone();
    report(
        &control.event_ingest_service(),
        &first,
        &runtime,
        &turn,
        EventType::TurnStarted,
    )
    .await;

    let rejectable = control
        .interactions()
        .register(ChoiceInteractionRequest {
            authorization_digest: "a".repeat(64),
            report_key: "rejectable".into(),
            blocked: None,
            session_id: first.clone(),
            turn_id: Some(turn.clone()),
            title: "Approve command".into(),
            description: None,
            options: vec![
                InteractionOption {
                    key: "allow".into(),
                    label: "Allow".into(),
                    intent: InteractionIntent::Positive,
                },
                InteractionOption {
                    key: "deny".into(),
                    label: "Deny".into(),
                    intent: InteractionIntent::Negative,
                },
            ],
        })
        .await
        .unwrap();
    let positive_only = control
        .interactions()
        .register(ChoiceInteractionRequest {
            authorization_digest: "a".repeat(64),
            report_key: "positive_only".into(),
            blocked: None,
            session_id: first.clone(),
            turn_id: Some(turn.clone()),
            title: "Confirm action".into(),
            description: None,
            options: vec![InteractionOption {
                key: "allow-only".into(),
                label: "Allow".into(),
                intent: InteractionIntent::Positive,
            }],
        })
        .await
        .unwrap();
    control
        .event_ingest_service()
        .report_fact(ReportedFact {
            session_id: first.clone(),
            turn_id: Some(turn.clone()),
            report_key: None,
            fact_type: EventType::InteractionBlocked,
            data: json!({"runtime_id": runtime, "interaction_id": rejectable.interaction_id}),
        })
        .await
        .unwrap();
    assert_eq!(
        control
            .queries()
            .session_snapshot(&first)
            .await
            .unwrap()
            .unwrap()
            .state,
        "waiting_for_interaction"
    );

    assert!(dreaming.start().await.unwrap().is_none());
    assert_eq!(
        *client.interaction_responses.lock().unwrap(),
        vec![(
            first.clone(),
            rejectable.interaction_id.clone(),
            "deny".into()
        )]
    );
    assert_eq!(
        control
            .interactions()
            .get(&first, &positive_only.interaction_id)
            .await
            .unwrap()
            .state,
        InteractionState::Pending
    );
    assert!(!GenericRuntimeManager.is_alive(&runtime));

    control
        .event_ingest_service()
        .report_client_fact(
            &first,
            Some(&runtime),
            EventType::SessionExited,
            json!({"reason": "terminated blocked dreaming round"}),
        )
        .await
        .unwrap();

    let second = dreaming.start().await.unwrap().unwrap();
    assert_launch_mode(&control, &client, &second, LaunchMode::Background).await;
    assert_ne!(second, first);
    let shared = root.path().join("bot/readonly").canonicalize().unwrap();
    let directories = client.launch_directories.lock().unwrap().clone();
    assert_eq!(directories.get(&first).map(String::as_str), shared.to_str());
    assert_eq!(
        directories.get(&second).map(String::as_str),
        shared.to_str()
    );

    // Even if rejecting an interaction fails, Dreaming must still terminate the blocked round.
    *client.fail_interaction_response.lock().unwrap() = true;
    let snapshot = control
        .queries()
        .session_snapshot(&second)
        .await
        .unwrap()
        .unwrap();
    let second_runtime = snapshot.runtimes[0].runtime_id.clone();
    let second_turn = control.queries().list_turns(&second).await.unwrap()[0]
        .turn_id
        .clone();
    report(
        &control.event_ingest_service(),
        &second,
        &second_runtime,
        &second_turn,
        EventType::TurnStarted,
    )
    .await;
    let failing = control
        .interactions()
        .register(ChoiceInteractionRequest {
            authorization_digest: "a".repeat(64),
            report_key: "failing".into(),
            blocked: None,
            session_id: second.clone(),
            turn_id: Some(second_turn.clone()),
            title: "Approve command".into(),
            description: None,
            options: vec![InteractionOption {
                key: "deny".into(),
                label: "Deny".into(),
                intent: InteractionIntent::Negative,
            }],
        })
        .await
        .unwrap();
    control
        .event_ingest_service()
        .report_fact(ReportedFact {
            session_id: second.clone(),
            turn_id: Some(second_turn),
            report_key: None,
            fact_type: EventType::InteractionBlocked,
            data: json!({"runtime_id": second_runtime, "interaction_id": failing.interaction_id}),
        })
        .await
        .unwrap();

    assert!(dreaming.start().await.unwrap().is_none());
    assert!(!GenericRuntimeManager.is_alive(&second_runtime));
    control
        .event_ingest_service()
        .report_client_fact(
            &second,
            Some(&second_runtime),
            EventType::SessionExited,
            json!({"reason": "terminated blocked dreaming round after reject failure"}),
        )
        .await
        .unwrap();
    let third = dreaming.start().await.unwrap().unwrap();
    assert_launch_mode(&control, &client, &third, LaunchMode::Background).await;
    assert_ne!(third, second);
}

#[tokio::test]
async fn failed_launch_is_cleaned_up_before_the_next_round() {
    let (_root, pool, client, control, conversation) = fixture().await;
    let (_, bot_session) = bot(&conversation).await;
    seed_pending_memory(pool, &conversation, &bot_session).await;
    let dreaming = DreamingService::new(control.clone(), conversation);
    *client.fail_next_launch.lock().unwrap() = true;
    assert!(dreaming.start().await.is_err());
    let failed = latest_dreaming_session_id(&control).await.unwrap();
    assert_eq!(
        control
            .queries()
            .get_session(&failed)
            .await
            .unwrap()
            .unwrap()
            .state,
        "error"
    );
    let next = dreaming.start().await.unwrap().unwrap();
    assert_ne!(next, failed);
    assert_eq!(
        control
            .queries()
            .get_session(&failed)
            .await
            .unwrap()
            .unwrap()
            .state,
        "exited"
    );
}
