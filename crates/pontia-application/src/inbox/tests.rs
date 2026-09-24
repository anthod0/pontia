use super::*;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};

fn queued_message<'a>(
    message_id: &'a str,
    session_id: &'a str,
) -> pontia_storage_sqlite::repositories::inbox::NewInboxMessage<'a> {
    pontia_storage_sqlite::repositories::inbox::NewInboxMessage {
        message_id,
        session_id,
        delivery_policy: "after_idle",
        input: "input",
        metadata: "{}",
        branch_target: None,
        steer_target: None,
        retry_of: None,
        resuming: false,
    }
}

#[tokio::test]
async fn prepared_input_is_held_until_release_and_concurrent_release_delivers_once() {
    let (_root, state, channel) = connected_inbox().await;
    let inbox = state.inbox_commands();
    let sessions = state.session_commands();
    let a = inbox
        .prepare_message_once(
            &sessions,
            "prepared",
            "session",
            request("continue existing work"),
        )
        .await
        .unwrap();
    let b = inbox
        .prepare_message_once(
            &sessions,
            "prepared",
            "session",
            request("continue existing work"),
        )
        .await
        .unwrap();
    assert_eq!(a.data["inbox_message"]["state"], "resuming");
    assert!(b.duplicate);
    inbox.drain_inbox("session").await.unwrap();
    assert!(channel.input.lock().unwrap().is_empty());
    let (a, b) = tokio::join!(
        inbox.release_prepared_message("session", "prepared", "runtime"),
        inbox.release_prepared_message("session", "prepared", "runtime")
    );
    a.unwrap();
    b.unwrap();
    assert_eq!(
        *channel.input.lock().unwrap(),
        vec!["continue existing work"]
    );
}

#[tokio::test]
async fn prepared_input_after_restart_or_unknown_delivery_is_never_replayed() {
    for interrupted in [true, false] {
        let (_root, state, channel) = connected_inbox().await;
        let inbox = state.inbox_commands();
        inbox
            .prepare_message_once(
                &state.session_commands(),
                "prepared",
                "session",
                request("recover"),
            )
            .await
            .unwrap();
        if interrupted {
            inbox.recover_deliveries().await.unwrap();
        } else {
            *channel.next_error.lock().unwrap() =
                Some(Error::ControlUnknown("lost receipt".into()));
        }
        inbox
            .release_prepared_message("session", "prepared", "runtime")
            .await
            .unwrap();
        inbox
            .release_prepared_message("session", "prepared", "runtime")
            .await
            .unwrap();
        let message = inbox
            .get_message("session", "prepared")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            message.state,
            if interrupted { "failed" } else { "unknown" }
        );
        assert_eq!(
            channel.input.lock().unwrap().len(),
            usize::from(!interrupted)
        );
    }
}

#[tokio::test]
async fn pending_prepared_input_cannot_follow_a_replacement_runtime_after_restart() {
    let (root, state, old_channel) = connected_inbox().await;
    let inbox = state.inbox_commands();
    inbox
        .prepare_message_once(
            &state.session_commands(),
            "prepared",
            "session",
            request("recover"),
        )
        .await
        .unwrap();
    *old_channel.next_error.lock().unwrap() = Some(Error::Conflict {
        code: "input_busy",
        message: "became busy".into(),
    });
    inbox
        .release_prepared_message("session", "prepared", "runtime")
        .await
        .unwrap();
    assert_eq!(
        inbox
            .get_message("session", "prepared")
            .await
            .unwrap()
            .unwrap()
            .state,
        "pending"
    );

    sqlx::query(
        "UPDATE runtime_bindings SET runtime_instance_id='replacement' WHERE session_id='session'",
    )
    .execute(&state.db())
    .await
    .unwrap();
    let restarted = crate::AppState::builder(state.db(), root.path().into())
        .clients(crate::clients::testing::clients())
        .build();
    let replacement = crate::clients::testing::channel();
    restarted
        .client_control()
        .attach(
            "test-channel",
            "session",
            "replacement",
            "native",
            replacement.clone(),
        )
        .await
        .unwrap();
    let recovered_inbox = restarted.inbox_commands();
    recovered_inbox.recover_deliveries().await.unwrap();
    recovered_inbox.resume_pending().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let message = recovered_inbox
                .get_message("session", "prepared")
                .await
                .unwrap()
                .unwrap();
            if message.state == "failed" {
                assert!(message.failure_message.unwrap().contains("runtime"));
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(old_channel.input.lock().unwrap().is_empty());
    assert!(replacement.input.lock().unwrap().is_empty());
    recovered_inbox
        .release_prepared_message("session", "prepared", "runtime")
        .await
        .unwrap();
    assert!(replacement.input.lock().unwrap().is_empty());
}

#[tokio::test]
async fn recovery_preserves_uncertainty_and_only_unsent_messages_can_be_claimed() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("test.db").display()
    ))
    .await
    .unwrap();
    run_migrations(&pool).await.unwrap();
    for client in ["test-channel", "codex"] {
        sqlx::query("INSERT INTO sessions(session_id,client_type,state) VALUES (?,?,'idle')")
            .bind(client)
            .bind(client)
            .execute(&pool)
            .await
            .unwrap();
        let repository = SqliteInboxRepository::new(pool.clone());
        for suffix in ["one", "two"] {
            repository
                .enqueue(queued_message(&format!("{client}-{suffix}"), client))
                .await
                .unwrap();
        }
        assert_eq!(
            repository
                .mark_dispatching(&format!("{client}-one"))
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            repository
                .mark_dispatching(&format!("{client}-two"))
                .await
                .unwrap(),
            0
        );
    }
    let state = crate::AppState::builder(pool.clone(), root.path().into())
        .clients(crate::clients::testing::clients())
        .build();
    let service = state.inbox_commands();
    service.recover_deliveries().await.unwrap();
    service.recover_deliveries().await.unwrap();
    for client in ["test-channel", "codex"] {
        let first = service
            .get_message(client, &format!("{client}-one"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.state, "unknown");
        assert!(first.failure_message.unwrap().contains("uncertain"));
        let repository = SqliteInboxRepository::new(pool.clone());
        assert_eq!(
            repository
                .mark_dispatching(&format!("{client}-one"))
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            repository
                .mark_dispatching(&format!("{client}-two"))
                .await
                .unwrap(),
            0,
            "unresolved delivery must not be overtaken"
        );
        sqlx::query("INSERT INTO turns(turn_id,session_id,state) VALUES (?,?,'completed')")
            .bind(format!("{client}-turn"))
            .bind(client)
            .execute(&pool)
            .await
            .unwrap();
        repository
            .link_started_turn(client, &format!("{client}-one"), &format!("{client}-turn"))
            .await
            .unwrap();
        assert_eq!(
            service
                .get_message(client, &format!("{client}-one"))
                .await
                .unwrap()
                .unwrap()
                .state,
            "dispatched"
        );
        assert_eq!(
            repository
                .mark_dispatching(&format!("{client}-two"))
                .await
                .unwrap(),
            1
        );
        repository
            .mark_dispatched(&format!("{client}-two"), None)
            .await
            .unwrap();
        repository
            .enqueue(queued_message(&format!("{client}-three"), client))
            .await
            .unwrap();
        assert_eq!(
            repository
                .mark_dispatching(&format!("{client}-three"))
                .await
                .unwrap(),
            0,
            "wait for a fact associating accepted input with its Turn"
        );
    }
}

#[tokio::test]
async fn runtime_scoped_exit_and_interrupt_reject_replaced_instances_before_native_control() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("test.db").display()
    ))
    .await
    .unwrap();
    run_migrations(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO sessions(session_id,client_type,state) VALUES ('session','test-channel','idle')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO runtime_bindings(session_id,runtime_kind,runtime_instance_id,binding_state) VALUES ('session','pi_tui','replacement','confirmed')").execute(&pool).await.unwrap();
    let state = crate::AppState::builder(pool.clone(), root.path().into())
        .clients(crate::clients::testing::clients())
        .build();
    let session = state.session_commands();
    for error in [
        session
            .ensure_current_runtime("session", "old")
            .await
            .unwrap_err(),
        session
            .request_exit("session", Some("old"))
            .await
            .unwrap_err(),
        state
            .turn_commands()
            .interrupt_turn_for_runtime("session", "turn", "old")
            .await
            .unwrap_err(),
    ] {
        assert!(matches!(error, Error::StateConflict(_)));
    }
    let error = state
        .turn_commands()
        .dispatch_initial(
            &crate::runtime::ControlTarget {
                session_id: "session".into(),
                runtime_instance_id: Some("old".into()),
            },
            "initial input",
            &json!({}),
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(error, Error::StateConflict(_)));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn token_override_preserves_initial_input_gate_and_event_wakeup() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let state = crate::AppState::builder(pool.clone(), root.path().into())
        .clients(crate::clients::testing::clients())
        .build();
    sqlx::query("INSERT INTO sessions(session_id,client_type,state) VALUES ('session','test-channel','idle')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO runtime_bindings(session_id,runtime_kind,runtime_instance_id,binding_state,tmux_socket_path,tmux_pane_id,capabilities) VALUES ('session','test_tui','runtime','confirmed','/unused/tmux','%1','{\"accept_task\":true}')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO agent_bindings(id,session_id,client_type,launch_cwd,client_session_key,metadata) VALUES ('binding','session','test-channel','/unused','native','{}')").execute(&pool).await.unwrap();
    let channel = crate::clients::testing::channel();
    state
        .client_control()
        .attach(
            "test-channel",
            "session",
            "runtime",
            "native",
            channel.clone(),
        )
        .await
        .unwrap();
    state
        .event_ingest_service()
        .report_fact(crate::ReportedFact {
            session_id: "session".into(),
            turn_id: None,
            fact_type: pontia_core::domain::EventType::SessionReady,
            data: json!({"runtime_instance_id":"runtime"}),
        })
        .await
        .unwrap();
    state.inbox_commands().scheduler.begin_initial("session");
    let authenticated = state.with_external_api_token(Some("token".into()));
    let submitted = authenticated
        .inbox_commands()
        .submit_message(
            "session",
            SubmitInboxMessageRequest {
                input: "queued".into(),
                delivery_policy: "after_idle".into(),
                branch_target_turn_id: None,
                metadata: json!({}),
            },
        )
        .await
        .unwrap();
    assert_eq!(submitted.data["inbox_message"]["state"], "pending");
    assert!(channel.input.lock().unwrap().is_empty());
    let id = submitted.data["inbox_message"]["message_id"]
        .as_str()
        .unwrap();
    state.inbox_commands().scheduler.finish_initial("session");
    authenticated
        .event_ingest_service()
        .control_available("session");
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if authenticated
                .inbox_commands()
                .get_message("session", id)
                .await
                .unwrap()
                .unwrap()
                .state
                == "dispatched"
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(*channel.input.lock().unwrap(), ["queued"]);
    authenticated.inbox_commands().stop_scheduling().await;
}

async fn connected_inbox() -> (
    tempfile::TempDir,
    crate::AppState,
    std::sync::Arc<crate::clients::testing::Channel>,
) {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let mut clients = crate::clients::testing::clients();
    let mut registration = clients.get("test-channel").unwrap().clone();
    registration.steer = true;
    clients.register(registration);
    let state = crate::AppState::builder(pool.clone(), root.path().into())
        .clients(clients)
        .build();
    sqlx::query("INSERT INTO sessions(session_id,client_type,state) VALUES ('session','test-channel','idle')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO runtime_bindings(session_id,runtime_kind,runtime_instance_id,binding_state,tmux_socket_path,tmux_pane_id,capabilities) VALUES ('session','test_tui','runtime','confirmed','/unused/tmux','%1','{\"accept_task\":true}')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO agent_bindings(id,session_id,client_type,launch_cwd,client_session_key,metadata) VALUES ('binding','session','test-channel','/unused','native','{}')").execute(&pool).await.unwrap();
    let channel = crate::clients::testing::channel();
    state
        .client_control()
        .attach(
            "test-channel",
            "session",
            "runtime",
            "native",
            channel.clone(),
        )
        .await
        .unwrap();
    state
        .event_ingest_service()
        .report_fact(crate::ReportedFact {
            session_id: "session".into(),
            turn_id: None,
            fact_type: pontia_core::domain::EventType::SessionReady,
            data: json!({"runtime_instance_id":"runtime"}),
        })
        .await
        .unwrap();
    state.inbox_commands().stop_scheduling().await;
    (root, state, channel)
}

fn request(input: &str) -> SubmitInboxMessageRequest {
    SubmitInboxMessageRequest {
        input: input.into(),
        delivery_policy: "after_idle".into(),
        branch_target_turn_id: None,
        metadata: json!({}),
    }
}

#[tokio::test]
async fn busy_rejection_preserves_fifo_and_cancelled_input_never_enters_client() {
    let (_root, state, channel) = connected_inbox().await;
    let inbox = state.inbox_commands();
    inbox.scheduler.begin_initial("session");
    for index in 0..13 {
        // Reverse identifiers deliberately: arrival order must break timestamp ties.
        inbox
            .submit_message_once(
                &format!("msg-{:02}", 13 - index),
                "session",
                request(&index.to_string()),
            )
            .await
            .unwrap();
    }
    inbox.cancel_message("session", "msg-07").await.unwrap();
    inbox.scheduler.finish_initial("session");
    *channel.next_error.lock().unwrap() = Some(Error::Conflict {
        code: "input_busy",
        message: "became busy".into(),
    });
    inbox.drain_inbox("session").await.unwrap();
    assert_eq!(
        inbox
            .get_message("session", "msg-13")
            .await
            .unwrap()
            .unwrap()
            .state,
        "pending"
    );
    assert!(channel.input.lock().unwrap().is_empty());
    for index in (0..13).filter(|index| *index != 6) {
        inbox.drain_inbox("session").await.unwrap();
        assert_eq!(
            channel.input.lock().unwrap().last(),
            Some(&index.to_string())
        );
        let turn = format!("turn-{index}");
        sqlx::query("INSERT INTO turns(turn_id,session_id,state) VALUES (?,'session','completed')")
            .bind(&turn)
            .execute(&state.db())
            .await
            .unwrap();
        SqliteInboxRepository::new(state.db())
            .link_started_turn("session", &format!("msg-{:02}", 13 - index), &turn)
            .await
            .unwrap();
    }
    assert_eq!(
        *channel.input.lock().unwrap(),
        (0..13)
            .filter(|index| *index != 6)
            .map(|index| index.to_string())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn unknown_delivery_requires_explicit_new_execution_and_retains_retry_lineage() {
    let (_root, state, channel) = connected_inbox().await;
    let inbox = state.inbox_commands();
    *channel.next_error.lock().unwrap() = Some(Error::ControlUnknown("response lost".into()));
    let first = inbox
        .submit_message_once("first", "session", request("same text"))
        .await
        .unwrap();
    assert_eq!(first.data["inbox_message"]["state"], "unknown");
    inbox
        .submit_message_once("first", "session", request("same text"))
        .await
        .unwrap();
    inbox.drain_inbox("session").await.unwrap();
    assert_eq!(channel.input.lock().unwrap().len(), 1);
    assert!(
        inbox
            .retry_message(
                &state.session_commands(),
                "session",
                "first",
                RetryInboxMessageRequest {
                    message_id: "retry".into(),
                    allow_unknown: false
                }
            )
            .await
            .is_err()
    );
    inbox
        .retry_message(
            &state.session_commands(),
            "session",
            "first",
            RetryInboxMessageRequest {
                message_id: "retry".into(),
                allow_unknown: true,
            },
        )
        .await
        .unwrap();
    let repeated = inbox
        .retry_message(
            &state.session_commands(),
            "session",
            "first",
            RetryInboxMessageRequest {
                message_id: "double-click".into(),
                allow_unknown: true,
            },
        )
        .await
        .unwrap();
    assert!(repeated.duplicate);
    assert_eq!(channel.input.lock().unwrap().len(), 2);
    let original = inbox
        .get_message("session", "first")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(original.state, "unknown");
    assert_eq!(original.retried_by_message_id.as_deref(), Some("retry"));
    assert_eq!(
        inbox
            .get_message("session", "retry")
            .await
            .unwrap()
            .unwrap()
            .retry_of_message_id
            .as_deref(),
        Some("first")
    );
}

#[tokio::test]
async fn queued_steer_never_retargets_a_different_turn() {
    let (_root, state, channel) = connected_inbox().await;
    let inbox = state.inbox_commands();
    sqlx::query(
        "INSERT INTO turns(turn_id,session_id,state) VALUES ('original-turn','session','running')",
    )
    .execute(&state.db())
    .await
    .unwrap();
    inbox.scheduler.begin_initial("session");
    let mut input = request("steer original");
    input.delivery_policy = "steer".into();
    inbox
        .submit_message_once("steer", "session", input)
        .await
        .unwrap();
    sqlx::query("UPDATE turns SET state='completed' WHERE turn_id='original-turn'")
        .execute(&state.db())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO turns(turn_id,session_id,state) VALUES ('next-turn','session','running')",
    )
    .execute(&state.db())
    .await
    .unwrap();
    inbox.scheduler.finish_initial("session");
    inbox.drain_inbox("session").await.unwrap();
    let message = inbox
        .get_message("session", "steer")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(message.state, "failed");
    assert_eq!(
        message.steer_target_turn_id.as_deref(),
        Some("original-turn")
    );
    assert!(message.failure_message.unwrap().contains("changed"));
    assert!(channel.input.lock().unwrap().is_empty());
}

#[tokio::test]
async fn failed_session_restore_is_diagnostic_and_repeated_retry_does_not_restart() {
    let (_root, state, channel) = connected_inbox().await;
    sqlx::query("UPDATE sessions SET state='exited' WHERE session_id='session'")
        .execute(&state.db())
        .await
        .unwrap();
    sqlx::query("INSERT INTO inbox_messages(message_id,session_id,state,delivery_policy,input_summary) VALUES ('failure','session','failed','after_idle','recover')").execute(&state.db()).await.unwrap();
    let inbox = state.inbox_commands();
    let first = inbox
        .retry_message(
            &state.session_commands(),
            "session",
            "failure",
            RetryInboxMessageRequest {
                message_id: "restore".into(),
                allow_unknown: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(first.data["inbox_message"]["state"], "failed");
    assert!(
        first.data["inbox_message"]["failure_message"]
            .as_str()
            .unwrap()
            .contains("Session recovery failed")
    );
    let repeat = inbox
        .retry_message(
            &state.session_commands(),
            "session",
            "failure",
            RetryInboxMessageRequest {
                message_id: "another".into(),
                allow_unknown: false,
            },
        )
        .await
        .unwrap();
    assert!(repeat.duplicate);
    assert_eq!(repeat.data, first.data);
    assert!(channel.input.lock().unwrap().is_empty());
    let starts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_type='session.resuming'")
            .fetch_one(&state.db())
            .await
            .unwrap();
    assert_eq!(starts, 1);
}

#[tokio::test]
async fn duplicate_submission_preserves_native_receipt_and_original_contents() {
    let (_root, state, channel) = connected_inbox().await;
    let inbox = state.inbox_commands();
    let mut original = request("first input");
    original.metadata = json!(["caller metadata"]);
    inbox
        .submit_message_once("message", "session", original.clone())
        .await
        .unwrap();
    InboxAssociations::new(state.db())
        .record_receipt(
            "session",
            "message",
            &crate::control::InputReceipt {
                native_turn_id: Some("native-turn".into()),
                runtime_instance_id: Some("runtime".into()),
            },
        )
        .await
        .unwrap();
    let current = inbox
        .get_message("session", "message")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current.metadata, json!({"codex_turn_id":"native-turn"}));
    for repeated in [
        original,
        SubmitInboxMessageRequest {
            input: "changed input".into(),
            delivery_policy: "interrupt_now".into(),
            branch_target_turn_id: Some("different-target".into()),
            metadata: json!({"changed":true}),
        },
    ] {
        let duplicate = inbox
            .submit_message_once("message", "session", repeated)
            .await
            .unwrap();
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.data["inbox_message"], json!(current));
    }
    assert_eq!(*channel.input.lock().unwrap(), ["first input"]);
    assert_eq!(inbox.list_messages("session").await.unwrap().len(), 1);
}

#[tokio::test]
async fn submission_identity_still_enforces_session_and_retry_associations() {
    let (_root, state, channel) = connected_inbox().await;
    let inbox = state.inbox_commands();
    inbox
        .submit_message_once("message", "session", request("input"))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO sessions(session_id,client_type,state) VALUES ('other','test-channel','idle')",
    )
    .execute(&state.db())
    .await
    .unwrap();
    let cross_session = inbox
        .submit_message_once("message", "other", request("input"))
        .await
        .unwrap_err();
    assert!(matches!(cross_session, Error::StateConflict(_)));
    sqlx::query("INSERT INTO inbox_messages(message_id,session_id,state,delivery_policy,input_summary) VALUES ('failed','session','failed','after_idle','input')")
        .execute(&state.db()).await.unwrap();
    let unrelated_retry = inbox
        .retry_message(
            &state.session_commands(),
            "session",
            "failed",
            RetryInboxMessageRequest {
                message_id: "message".into(),
                allow_unknown: false,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(unrelated_retry, Error::StateConflict(_)));
    let failed = inbox
        .get_message("session", "failed")
        .await
        .unwrap()
        .unwrap();
    assert!(failed.retried_by_message_id.is_none());
    assert_eq!(*channel.input.lock().unwrap(), ["input"]);
}
