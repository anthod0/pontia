use super::*;
use pontia_bot::{
    dreaming::{DreamingRequest, DreamingService},
    identity::BotRole,
    memory::{MemoryCommands, MemoryService},
};

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
    finish(&control, &first, EventType::TurnFailed).await;
    let restarted = DreamingService::new(control.clone(), conversation.clone());
    assert!(restarted.start().await.unwrap().is_none());
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
            json!({"reason":"test client exit"}),
        )
        .await
        .unwrap();
    let second = restarted.start().await.unwrap().unwrap();
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
    assert!(
        control
            .queries()
            .latest_owned_session("bot", "dreaming:")
            .await
            .unwrap()
            .is_none()
    );
    tokio::time::advance(std::time::Duration::from_secs(1)).await;
    // Database and launch work run on real worker threads, independent of the paused clock.
    tokio::time::resume();
    let first = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(id) = control
                .queries()
                .latest_owned_session("bot", "dreaming:")
                .await
                .unwrap()
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
        control
            .queries()
            .latest_owned_session("bot", "dreaming:")
            .await
            .unwrap()
            .as_deref(),
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
        control
            .queries()
            .latest_owned_session("bot", "dreaming:")
            .await
            .unwrap()
            .as_deref(),
        Some(first.as_str())
    );
    tokio::time::advance(std::time::Duration::from_secs(10)).await;
    tokio::time::resume();
    let next = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(id) = control
                .queries()
                .latest_owned_session("bot", "dreaming:")
                .await
                .unwrap()
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
    let restart = DreamingService::new(control.clone(), ConversationService::new(control.clone()));
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
