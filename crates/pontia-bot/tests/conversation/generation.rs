use super::*;
use pontia_bot::conversation::GenerationLimits;
use time::{Duration, OffsetDateTime};

async fn usage(control: &AgentControl, session: &str, used: u64, window: u64) {
    let snapshot = control
        .queries()
        .session_snapshot(session)
        .await
        .unwrap()
        .unwrap();
    control.event_ingest_service().report_client_fact(
        session, Some(&snapshot.runtimes[0].runtime_id), EventType::SessionContextUsageUpdated,
        json!({"context_usage":{"used_tokens":used,"max_tokens":window,"confidence":"estimated"}}),
    ).await.unwrap();
}

async fn current(service: &ConversationService, bot: &str) -> Option<String> {
    service.records().get(bot).await.unwrap().current_session_id
}

async fn reconcile(service: &ConversationService) {
    service
        .reconcile_generations(OffsetDateTime::now_utc())
        .await
        .unwrap();
}

#[tokio::test]
async fn both_upper_limits_wait_for_idle_and_keep_helpers_and_work_status() {
    for (limit, window) in [(258_000, 1_000_000), (102_400, 128_000)] {
        let (_root, _pool, client, control, conversation) = fixture().await;
        let _exits = report_exits(&control);
        let (id, old) = bot(&conversation).await;
        let helpers = HelperService::new(control.clone(), conversation.clone());
        let worker = helper(&helpers, &old).await;
        conversation
            .send_to_user(ReplyRequest {
                session_id: old.clone(),
                input: Reply {
                    message: "working".into(),
                    working_on: Some("background task".into()),
                },
            })
            .await
            .unwrap();
        usage(&control, &old, limit, window).await;
        finish(&control, &old, EventType::TurnCompleted).await;
        reconcile(&conversation).await;
        assert_eq!(
            current(&conversation, &id).await.as_deref(),
            Some(old.as_str())
        );
        conversation
            .send(&id, "already accepted by old client")
            .await
            .unwrap();
        usage(&control, &old, limit + 1, window).await;
        reconcile(&conversation).await;
        assert_eq!(
            current(&conversation, &id).await.as_deref(),
            Some(old.as_str())
        );
        finish(&control, &old, EventType::TurnCompleted).await;
        let recovered = test_conversation(control.clone());
        reconcile(&recovered).await;
        let new = current(&recovered, &id).await.unwrap();
        assert_ne!(new, old);
        assert_eq!(
            control
                .queries()
                .get_session(&old)
                .await
                .unwrap()
                .unwrap()
                .state,
            "exited"
        );
        assert_eq!(
            recovered.chat(&id).await.unwrap().bot.working_on.as_deref(),
            Some("background task")
        );
        assert_eq!(helpers.list(&id).await.unwrap()[0].session_id, worker);
        assert_eq!(
            client
                .inputs
                .lock()
                .unwrap()
                .iter()
                .filter(|input| input.session_id == new)
                .count(),
            0
        );
        recovered.send(&id, "after handoff").await.unwrap();
        let inputs = client.inputs.lock().unwrap();
        assert_eq!(inputs.last().unwrap().session_id, new);
        assert_eq!(
            inputs
                .iter()
                .filter(|input| input.input.contains("already accepted by old client"))
                .count(),
            1
        );
    }
}

#[tokio::test]
async fn idle_timer_resets_on_execution_and_helper_wakes_a_new_generation() {
    let (_root, _pool, client, control, conversation) = fixture().await;
    let _exits = report_exits(&control);
    let (id, old) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let worker = helper(&helpers, &old).await;
    usage(&control, &old, 75_000, 1_000_000).await;
    finish(&control, &old, EventType::TurnCompleted).await;
    conversation
        .reconcile_generations(OffsetDateTime::now_utc() + Duration::minutes(6))
        .await
        .unwrap();
    assert_eq!(
        current(&conversation, &id).await.as_deref(),
        Some(old.as_str())
    );
    usage(&control, &old, 75_001, 1_000_000).await;
    conversation
        .reconcile_generations(OffsetDateTime::now_utc() + Duration::seconds(299))
        .await
        .unwrap();
    assert_eq!(
        current(&conversation, &id).await.as_deref(),
        Some(old.as_str())
    );
    conversation.send(&id, "interrupt idle").await.unwrap();
    conversation
        .reconcile_generations(OffsetDateTime::now_utc() + Duration::minutes(6))
        .await
        .unwrap();
    assert_eq!(
        current(&conversation, &id).await.as_deref(),
        Some(old.as_str())
    );
    finish(&control, &old, EventType::TurnCompleted).await;
    reconcile(&conversation).await;
    assert_eq!(
        current(&conversation, &id).await.as_deref(),
        Some(old.as_str())
    );
    conversation
        .reconcile_generations(OffsetDateTime::now_utc() + Duration::minutes(5))
        .await
        .unwrap();
    assert_eq!(current(&conversation, &id).await, None);
    let before = client.inputs.lock().unwrap().len();
    let recovered = test_conversation(control.clone());
    reconcile(&recovered).await;
    assert_eq!(client.inputs.lock().unwrap().len(), before);
    let helpers = HelperService::new(control.clone(), recovered.clone());
    let reply = helpers
        .execute(HelperRequest {
            session_id: worker,
            command: HelperCommand::Reply {
                message: "finished".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(reply["delivery"]["status"], "accepted");
    let new = current(&recovered, &id).await.unwrap();
    assert_ne!(new, old);
    assert_eq!(
        client.inputs.lock().unwrap().last().unwrap().session_id,
        new
    );
}

#[tokio::test]
async fn launch_failure_keeps_old_pointer_and_failed_input_requires_explicit_retry() {
    let (_root, _pool, client, control, conversation) = fixture().await;
    let _exits = report_exits(&control);
    let (id, old) = bot(&conversation).await;
    usage(&control, &old, 300_000, 1_000_000).await;
    finish(&control, &old, EventType::TurnCompleted).await;
    conversation
        .records()
        .update(&id, bot_configuration("new configuration", "missing/model"))
        .await
        .unwrap();
    reconcile(&conversation).await;
    assert!(!conversation.chat(&id).await.unwrap().notices.is_empty());
    let failed = conversation
        .send(&id, "not automatically replayed")
        .await
        .unwrap();
    assert_eq!(failed.status, MessageStatus::Failed);
    assert_eq!(
        current(&conversation, &id).await.as_deref(),
        Some(old.as_str())
    );
    conversation
        .records()
        .update(&id, bot_configuration("new configuration", MODEL))
        .await
        .unwrap();
    let recovered = test_conversation(control.clone());
    reconcile(&recovered).await;
    let new = current(&recovered, &id).await.unwrap();
    assert_ne!(new, old);
    assert_eq!(
        control
            .queries()
            .get_session(&new)
            .await
            .unwrap()
            .unwrap()
            .title
            .as_deref(),
        Some("new configuration")
    );
    assert_eq!(client.inputs.lock().unwrap().len(), 1);
    assert!(
        instructions(&control, &new)
            .await
            .contains("not automatically replayed")
    );
    assert_eq!(
        recovered.retry(&id, &failed.id).await.unwrap().status,
        MessageStatus::Normal
    );
    assert_eq!(
        client.inputs.lock().unwrap().last().unwrap().session_id,
        new
    );
}

fn launch_gate(
    client: &Client,
) -> (
    tokio::sync::oneshot::Receiver<()>,
    std::sync::mpsc::Sender<()>,
) {
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel();
    *client.launch_gate.lock().unwrap() = Some((entered, released));
    (waiting, release)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn user_and_helper_inputs_arriving_during_launch_go_only_to_new_session() {
    let (_root, _pool, client, control, conversation) = fixture().await;
    let _exits = report_exits(&control);
    let (id, old) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let worker = helper(&helpers, &old).await;
    usage(&control, &old, 300_000, 1_000_000).await;
    finish(&control, &old, EventType::TurnCompleted).await;
    let (waiting, release) = launch_gate(&client);
    let runner = conversation.clone();
    let rotating = tokio::spawn(async move { reconcile(&runner).await });
    waiting.await.unwrap();
    let sender = conversation.clone();
    let target = id.clone();
    let mut user =
        tokio::spawn(async move { sender.send(&target, "during switch").await.unwrap() });
    let mut reply = tokio::spawn(async move {
        helpers
            .execute(HelperRequest {
                session_id: worker,
                command: HelperCommand::Reply {
                    message: "during switch".into(),
                },
            })
            .await
            .unwrap()
    });
    while conversation.chat(&id).await.unwrap().messages.len() < 2 {
        tokio::task::yield_now().await;
    }
    // Poll both tasks while launch is held: neither may complete by routing to the old client.
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut user)
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut reply)
            .await
            .is_err()
    );
    release.send(()).unwrap();
    rotating.await.unwrap();
    assert_eq!(user.await.unwrap().status, MessageStatus::Normal);
    assert_eq!(reply.await.unwrap()["delivery"]["status"], "accepted");
    let new = current(&conversation, &id).await.unwrap();
    assert_ne!(new, old);
    let inputs = client.inputs.lock().unwrap();
    let delivered: Vec<_> = inputs
        .iter()
        .filter(|input| input.input.contains("during switch"))
        .collect();
    assert_eq!(delivered.len(), 2);
    assert!(delivered.iter().all(|input| input.session_id == new));
}

#[tokio::test]
async fn recovery_retires_uncommitted_sessions_and_never_revives_them_from_an_empty_pointer() {
    let (_root, _pool, client, control, conversation) = fixture().await;
    let _exits = report_exits(&control);
    let (id, orphan) = bot(&conversation).await;
    finish(&control, &orphan, EventType::TurnCompleted).await;
    // The same persisted state as a crash after launch and before committing the pointer.
    conversation
        .records()
        .set_current_session(&id, None)
        .await
        .unwrap();
    let recovered = test_conversation(control.clone());
    reconcile(&recovered).await;
    assert_eq!(current(&recovered, &id).await, None);
    assert_eq!(
        control
            .queries()
            .get_session(&orphan)
            .await
            .unwrap()
            .unwrap()
            .state,
        "exited"
    );
    recovered.send(&id, "new request").await.unwrap();
    let new = current(&recovered, &id).await.unwrap();
    assert_ne!(new, orphan);
    assert_eq!(client.inputs.lock().unwrap().len(), 2);
    assert_eq!(
        client.inputs.lock().unwrap().last().unwrap().session_id,
        new
    );
}

#[tokio::test]
async fn configured_upper_limits_apply_without_changing_the_idle_lower_limit() {
    let (_root, _pool, _client, control, conversation) = fixture().await;
    let _exits = report_exits(&control);
    assert!(GenerationLimits::new(0, 80).is_err());
    assert!(GenerationLimits::new(100_000, 0).is_err());
    assert!(GenerationLimits::new(100_000, 101).is_err());
    let conversation =
        conversation.with_generation_limits(GenerationLimits::new(100_000, 50).unwrap());
    let (id, old) = bot(&conversation).await;
    usage(&control, &old, 64_001, 128_000).await;
    finish(&control, &old, EventType::TurnCompleted).await;
    reconcile(&conversation).await;
    assert_ne!(
        current(&conversation, &id).await.as_deref(),
        Some(old.as_str())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resumed_execution_rejects_the_switch_and_recovery_cleans_the_unused_replacement() {
    let (_root, _pool, _client, control, conversation) = fixture().await;
    let _exits = report_exits(&control);
    let (id, old) = bot(&conversation).await;
    usage(&control, &old, 300_000, 1_000_000).await;
    finish(&control, &old, EventType::TurnCompleted).await;
    let (waiting, release) = launch_gate(&_client);
    let runner = conversation.clone();
    let rotating = tokio::spawn(async move { reconcile(&runner).await });
    waiting.await.unwrap();
    control
        .turn_commands()
        .submit_live_input(&old, "already in native queue".into())
        .await
        .unwrap();
    release.send(()).unwrap();
    rotating.await.unwrap();
    assert_eq!(
        current(&conversation, &id).await.as_deref(),
        Some(old.as_str())
    );
    assert!(conversation.chat(&id).await.unwrap().notices.is_empty());
    let owned = control.queries().list_owned_sessions("bot").await.unwrap();
    assert_eq!(owned.len(), 2);
    let unused = owned.iter().find(|session| **session != old).unwrap();
    assert_eq!(
        control
            .queries()
            .get_session(unused)
            .await
            .unwrap()
            .unwrap()
            .state,
        "exited"
    );
    let snapshot = control
        .queries()
        .session_snapshot(&old)
        .await
        .unwrap()
        .unwrap();
    assert!(GenericRuntimeManager.is_alive(&snapshot.runtimes[0].runtime_id));
    finish(&control, &old, EventType::TurnCompleted).await;
    reconcile(&test_conversation(control.clone())).await;
    assert_ne!(
        current(&conversation, &id).await.as_deref(),
        Some(old.as_str())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failed_handoff_preserves_helper_failure_notice_and_never_replays_original_report() {
    let (_root, pool, client, control, conversation) = fixture().await;
    let _exits = report_exits(&control);
    let (id, old) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let worker = helper(&helpers, &old).await;
    usage(&control, &old, 300_000, 1_000_000).await;
    finish(&control, &old, EventType::TurnCompleted).await;
    conversation
        .records()
        .update(&id, bot_configuration("Bot", "missing/model"))
        .await
        .unwrap();
    let (waiting, release) = launch_gate(&client);
    let runner = conversation.clone();
    let rotating = tokio::spawn(async move { reconcile(&runner).await });
    waiting.await.unwrap();
    let sender = conversation.clone();
    let target = id.clone();
    let mut user = tokio::spawn(async move { sender.send(&target, "waiting user").await.unwrap() });
    let worker_id = worker.clone();
    let mut reply = tokio::spawn(async move {
        helpers
            .execute(HelperRequest {
                session_id: worker_id,
                command: HelperCommand::Reply {
                    message: "original helper result".into(),
                },
            })
            .await
            .unwrap()
    });
    while conversation.chat(&id).await.unwrap().messages.len() < 2 {
        tokio::task::yield_now().await;
    }
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut user)
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut reply)
            .await
            .is_err()
    );
    release.send(()).unwrap();
    rotating.await.unwrap();
    let user = user.await.unwrap();
    assert_eq!(user.status, MessageStatus::Failed);
    assert_eq!(reply.await.unwrap()["delivery"]["status"], "failed");
    let pending: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bot_notifications WHERE delivered=0")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(pending, 1);
    conversation
        .records()
        .update(&id, bot_configuration("Bot", MODEL))
        .await
        .unwrap();
    let recovered = test_conversation(control.clone());
    reconcile(&recovered).await;
    let new = current(&recovered, &id).await.unwrap();
    let helpers = HelperService::new(control, recovered.clone());
    helpers.reconcile().await.unwrap();
    helpers.reconcile().await.unwrap();
    assert_eq!(
        recovered
            .chat(&id)
            .await
            .unwrap()
            .messages
            .iter()
            .find(|message| message.id == user.id)
            .unwrap()
            .status,
        MessageStatus::Failed
    );
    let inputs = client.inputs.lock().unwrap();
    assert_eq!(
        inputs
            .iter()
            .filter(|input| input.input.contains("original helper result"))
            .count(),
        0
    );
    let notifications: Vec<_> = inputs
        .iter()
        .filter(|input| input.session_id == new && input.input.starts_with("Source: System"))
        .collect();
    assert_eq!(notifications.len(), 1);
    assert!(notifications[0].input.contains(&worker));
    assert!(notifications[0].input.contains("model unavailable"));
}

#[tokio::test]
async fn unknown_exit_keeps_the_old_pointer_and_does_not_deliver_the_waiting_input() {
    let (_root, _pool, client, control, conversation) = fixture().await;
    let (id, old) = bot(&conversation).await;
    usage(&control, &old, 300_000, 1_000_000).await;
    finish(&control, &old, EventType::TurnCompleted).await;
    // No exit reporter: the runtime stops but its terminal fact is delayed.
    let result = conversation.deliver_input(&id, "trigger handoff").await;
    assert!(matches!(result, Err(Error::ControlUnknown(_))));
    assert_eq!(
        current(&conversation, &id).await.as_deref(),
        Some(old.as_str())
    );
    assert!(!conversation.chat(&id).await.unwrap().notices.is_empty());
    assert_eq!(client.inputs.lock().unwrap().len(), 1);

    let runtime = control
        .queries()
        .session_snapshot(&old)
        .await
        .unwrap()
        .unwrap()
        .runtimes[0]
        .runtime_id
        .clone();
    control
        .event_ingest_service()
        .report_client_fact(
            &old,
            Some(&runtime),
            EventType::SessionExited,
            json!({"reason": "delayed exit confirmation"}),
        )
        .await
        .unwrap();
    let _exits = report_exits(&control);
    let recovered = test_conversation(control.clone());
    reconcile(&recovered).await;
    assert_eq!(current(&recovered, &id).await, None);
    recovered.send(&id, "after confirmed exit").await.unwrap();
    let new = current(&recovered, &id).await.unwrap();
    assert_ne!(new, old);
    let inputs = client.inputs.lock().unwrap();
    assert_eq!(inputs.len(), 2);
    assert_eq!(inputs.last().unwrap().session_id, new);
}

#[tokio::test]
async fn helper_wake_launch_failure_is_visible_even_with_no_current_session() {
    let (_root, _pool, _client, control, conversation) = fixture().await;
    let _exits = report_exits(&control);
    let (id, old) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let worker = helper(&helpers, &old).await;
    usage(&control, &old, 80_000, 1_000_000).await;
    finish(&control, &old, EventType::TurnCompleted).await;
    reconcile(&conversation).await;
    conversation
        .reconcile_generations(OffsetDateTime::now_utc() + Duration::minutes(6))
        .await
        .unwrap();
    assert_eq!(current(&conversation, &id).await, None);
    conversation
        .records()
        .update(&id, bot_configuration("Bot", "missing/model"))
        .await
        .unwrap();
    let reply = helpers
        .execute(HelperRequest {
            session_id: worker,
            command: HelperCommand::Reply {
                message: "wake failed".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(reply["delivery"]["status"], "failed");
    let chat = conversation.chat(&id).await.unwrap();
    assert!(chat.bot.current_session_id.is_none());
    assert_eq!(chat.notices.len(), 1);
    assert!(
        chat.messages
            .iter()
            .all(|message| message.status == MessageStatus::Normal)
    );
}

#[tokio::test]
async fn recovery_releases_interrupted_and_error_orphan_runtimes() {
    for terminal in [EventType::TurnInterrupted, EventType::SessionError] {
        let (_root, _pool, _client, control, conversation) = fixture().await;
        let _exits = report_exits(&control);
        let (id, orphan) = bot(&conversation).await;
        if terminal == EventType::TurnInterrupted {
            finish(&control, &orphan, terminal).await;
        } else {
            let snapshot = control
                .queries()
                .session_snapshot(&orphan)
                .await
                .unwrap()
                .unwrap();
            control
                .event_ingest_service()
                .ingest_pontia_event(pontia_agent_control::PontiaEvent::new(
                    &orphan,
                    None,
                    pontia_agent_control::PontiaEventSource::RuntimeManager,
                    "generic",
                    pontia_agent_control::PontiaEventType::SessionError,
                    json!({"runtime_id":snapshot.runtimes[0].runtime_id,"reason":"test failure"}),
                ))
                .await
                .unwrap();
        }
        conversation
            .records()
            .set_current_session(&id, None)
            .await
            .unwrap();
        reconcile(&test_conversation(control.clone())).await;
        let snapshot = control
            .queries()
            .session_snapshot(&orphan)
            .await
            .unwrap()
            .unwrap();
        assert!(!GenericRuntimeManager.is_alive(&snapshot.runtimes[0].runtime_id));
        assert_eq!(current(&conversation, &id).await, None);
    }
}

#[tokio::test]
async fn helper_waiting_on_unknown_idle_exit_retains_uncertainty_and_is_not_replayed() {
    let (_root, pool, client, control, conversation) = fixture().await;
    let (_id, old) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let worker = helper(&helpers, &old).await;
    usage(&control, &old, 80_000, 1_000_000).await;
    finish(&control, &old, EventType::TurnCompleted).await;
    reconcile(&conversation).await;
    let runtime = control
        .queries()
        .session_snapshot(&old)
        .await
        .unwrap()
        .unwrap()
        .runtimes[0]
        .runtime_id
        .clone();
    let runner = conversation.clone();
    let rotating = tokio::spawn(async move {
        runner
            .reconcile_generations(OffsetDateTime::now_utc() + Duration::minutes(6))
            .await
            .unwrap();
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while GenericRuntimeManager.is_alive(&runtime) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let mut reply = Box::pin(helpers.execute(HelperRequest {
        session_id: worker.clone(),
        command: HelperCommand::Reply {
            message: "uncertain handoff reply".into(),
        },
    }));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut reply)
            .await
            .is_err()
    );
    rotating.await.unwrap();
    let result = reply.await.unwrap();
    assert_eq!(result["delivery"]["status"], "unknown");
    let notifications: Vec<(String, String)> =
        sqlx::query_as("SELECT source_session_id,content FROM bot_notifications WHERE delivered=0")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].0, worker);
    assert!(notifications[0].1.contains("unknown"));
    assert_eq!(
        client
            .inputs
            .lock()
            .unwrap()
            .iter()
            .filter(|input| input.input.contains("uncertain handoff reply"))
            .count(),
        0
    );
    let _exits = report_exits(&control);
    let recovered = test_conversation(control.clone());
    reconcile(&recovered).await;
    HelperService::new(control, recovered)
        .reconcile()
        .await
        .unwrap();
    assert_eq!(
        client
            .inputs
            .lock()
            .unwrap()
            .iter()
            .filter(|input| input.input.starts_with("Source: Helper"))
            .count(),
        0
    );
}

#[tokio::test]
async fn idle_timer_persists_across_restart_and_resets_after_fast_turn() {
    let (_root, _pool, _client, control, conversation) = fixture().await;
    let _exits = report_exits(&control);
    let (id, old) = bot(&conversation).await;
    let t0 = OffsetDateTime::now_utc();
    usage(&control, &old, 80_000, 1_000_000).await;
    finish(&control, &old, EventType::TurnCompleted).await;

    // First idle check records the persisted idle timer.
    conversation.reconcile_generations(t0).await.unwrap();
    assert_eq!(
        current(&conversation, &id).await.as_deref(),
        Some(old.as_str())
    );

    // Across restart, elapsed time continues from the persisted idle_since.
    let recovered = test_conversation(control.clone());
    recovered
        .reconcile_generations(t0 + Duration::seconds(299))
        .await
        .unwrap();
    assert_eq!(
        current(&recovered, &id).await.as_deref(),
        Some(old.as_str())
    );

    // A fast turn completing between checks changes current_turn_id and resets idle_since.
    recovered.send(&id, "fast turn").await.unwrap();
    finish(&control, &old, EventType::TurnCompleted).await;
    let t1 = t0 + Duration::minutes(6);
    recovered.reconcile_generations(t1).await.unwrap();
    assert_eq!(
        current(&recovered, &id).await.as_deref(),
        Some(old.as_str())
    );

    // Restarting again keeps counting from the reset timer and retires at 5 minutes.
    let restarted = test_conversation(control.clone());
    restarted
        .reconcile_generations(t1 + Duration::seconds(299))
        .await
        .unwrap();
    assert_eq!(
        current(&restarted, &id).await.as_deref(),
        Some(old.as_str())
    );
    restarted
        .reconcile_generations(t1 + Duration::minutes(5))
        .await
        .unwrap();
    assert_eq!(current(&restarted, &id).await, None);
}
