use super::*;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};

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
    sqlx::query(r#"INSERT INTO session_runtimes(session_id, runtime_id, role, state, created_at) VALUES ('session', 'replacement', 'generic_control', 'running', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))"#).execute(&pool).await.unwrap();
    let state = crate::AgentControl::builder(pool.clone(), root.path().into())
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
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn cloned_control_preserves_session_actor_and_event_wakeup() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let state = crate::AgentControl::builder(pool.clone(), root.path().into())
        .clients(crate::clients::testing::clients())
        .build();
    sqlx::query("INSERT INTO sessions(session_id,client_type,state) VALUES ('session','test-channel','idle')").execute(&pool).await.unwrap();
    sqlx::query(r#"INSERT INTO session_runtimes(session_id, runtime_id, tmux_socket_path, tmux_pane_id, role, state, created_at) VALUES ('session', 'runtime', '/unused/tmux', '%1', 'generic_control', 'running', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))"#).execute(&pool).await.unwrap();
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
            report_key: None,
            fact_type: pontia_core::domain::EventType::SessionReady,
            data: json!({"runtime_id":"runtime"}),
        })
        .await
        .unwrap();
    sqlx::query("UPDATE sessions SET state='starting' WHERE session_id='session'")
        .execute(&pool)
        .await
        .unwrap();
    let authenticated = state.clone();
    let submitted = authenticated
        .inbox_commands()
        .submit_message("session", request("queued"))
        .await
        .unwrap();
    assert_eq!(submitted.data["inbox_message"]["state"], "pending");
    assert!(channel.input.lock().unwrap().is_empty());
    let id = submitted.data["inbox_message"]["message_id"]
        .as_str()
        .unwrap();
    sqlx::query("UPDATE sessions SET state='idle' WHERE session_id='session'")
        .execute(&pool)
        .await
        .unwrap();
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
                .is_none()
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
    crate::AgentControl,
    std::sync::Arc<crate::clients::testing::Channel>,
) {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let state = crate::AgentControl::builder(pool.clone(), root.path().into())
        .clients(crate::clients::testing::clients())
        .build();
    sqlx::query("INSERT INTO sessions(session_id,client_type,state) VALUES ('session','test-channel','idle')").execute(&pool).await.unwrap();
    sqlx::query(r#"INSERT INTO session_runtimes(session_id, runtime_id, tmux_socket_path, tmux_pane_id, role, state, created_at) VALUES ('session', 'runtime', '/unused/tmux', '%1', 'generic_control', 'running', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))"#).execute(&pool).await.unwrap();
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
            report_key: None,
            fact_type: pontia_core::domain::EventType::SessionReady,
            data: json!({"runtime_id":"runtime"}),
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
        metadata: json!({}),
    }
}

#[tokio::test]
async fn immediate_send_delivers_without_creating_a_queue_row() {
    let (_root, state, channel) = connected_inbox().await;

    let outcome = state
        .inbox_commands()
        .send("session", "now".into(), crate::SendMode::Immediate)
        .await
        .unwrap();

    assert_eq!(outcome, crate::SendOutcome::Delivered);
    assert_eq!(*channel.input.lock().unwrap(), ["now"]);
    assert!(
        state
            .inbox_commands()
            .list_messages("session")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn uncertain_immediate_send_is_not_retried_or_queued() {
    let (_root, state, channel) = connected_inbox().await;
    *channel.next_error.lock().unwrap() = Some(Error::ControlUnknown("lost receipt".into()));

    let error = state
        .inbox_commands()
        .send("session", "once".into(), crate::SendMode::Immediate)
        .await
        .unwrap_err();

    assert!(matches!(error, crate::DeliveryError::Uncertain(_)));
    assert_eq!(*channel.input.lock().unwrap(), ["once"]);
    assert!(
        state
            .inbox_commands()
            .list_messages("session")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn successful_queued_send_deletes_its_row() {
    let (_root, state, channel) = connected_inbox().await;

    let outcome = state
        .inbox_commands()
        .send(
            "session",
            crate::InputPayload::from("queued").force_queue(),
            crate::SendMode::Queue,
        )
        .await
        .unwrap();

    assert_eq!(outcome, crate::SendOutcome::Delivered);
    assert_eq!(*channel.input.lock().unwrap(), ["queued"]);
    assert!(
        state
            .inbox_commands()
            .list_messages("session")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn offline_queued_send_remains_pending() {
    let (_root, state, channel) = connected_inbox().await;
    *channel.next_error.lock().unwrap() =
        Some(Error::CapabilityUnavailable("channel offline".into()));

    let crate::SendOutcome::Queued { message_id } = state
        .inbox_commands()
        .send(
            "session",
            crate::InputPayload::from("queued").force_queue(),
            crate::SendMode::Queue,
        )
        .await
        .unwrap()
    else {
        panic!("offline input must remain queued");
    };

    let message = state
        .inbox_commands()
        .get_message("session", &message_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(message.state, "pending");
}

#[tokio::test]
async fn state_changed_delivery_remains_pending_and_retries_once() {
    let (_root, state, channel) = connected_inbox().await;
    *channel.next_error.lock().unwrap() = Some(Error::Conflict {
        code: "input_busy",
        message: "became busy".into(),
    });

    let crate::SendOutcome::Queued { message_id } = state
        .inbox_commands()
        .send(
            "session",
            crate::InputPayload::from("queued").force_queue(),
            crate::SendMode::Queue,
        )
        .await
        .unwrap()
    else {
        panic!("state change must leave input queued");
    };

    assert!(channel.input.lock().unwrap().is_empty());
    assert!(
        state
            .inbox_commands()
            .get_message("session", &message_id)
            .await
            .unwrap()
            .is_some()
    );

    state.inbox_commands().drain_inbox("session").await.unwrap();

    assert_eq!(*channel.input.lock().unwrap(), ["queued"]);
    assert!(
        state
            .inbox_commands()
            .get_message("session", &message_id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn queued_messages_are_fifo_and_cancelled_input_is_not_sent() {
    let (_root, state, channel) = connected_inbox().await;
    let inbox = state.inbox_commands();
    sqlx::query("UPDATE sessions SET state='busy' WHERE session_id='session'")
        .execute(&state.db())
        .await
        .unwrap();
    for index in 0..3 {
        inbox
            .submit_message_once(
                &format!("message-{index}"),
                "session",
                request(&index.to_string()),
            )
            .await
            .unwrap();
    }
    inbox.cancel_message("session", "message-1").await.unwrap();
    sqlx::query("UPDATE sessions SET state='idle' WHERE session_id='session'")
        .execute(&state.db())
        .await
        .unwrap();

    for delivered in 0..2 {
        inbox.drain_inbox("session").await.unwrap();
        if delivered == 0 {
            state
                .event_ingest_service()
                .report_fact(crate::ReportedFact {
                    session_id: "session".into(),
                    turn_id: None,
                    report_key: None,
                    fact_type: pontia_core::domain::EventType::SessionReady,
                    data: json!({"runtime_id":"runtime"}),
                })
                .await
                .unwrap();
        }
    }

    assert_eq!(*channel.input.lock().unwrap(), ["0", "2"]);
    assert!(inbox.list_messages("session").await.unwrap().is_empty());
}

#[tokio::test]
async fn duplicate_pending_submission_preserves_original_contents() {
    let (_root, state, _channel) = connected_inbox().await;
    let inbox = state.inbox_commands();
    sqlx::query("UPDATE sessions SET state='busy' WHERE session_id='session'")
        .execute(&state.db())
        .await
        .unwrap();
    let mut original = request("first input");
    original.metadata = json!(["caller metadata"]);
    inbox
        .submit_message_once("message", "session", original)
        .await
        .unwrap();

    let duplicate = inbox
        .submit_message_once("message", "session", request("changed input"))
        .await
        .unwrap();

    assert!(duplicate.duplicate);
    assert_eq!(
        duplicate.data["inbox_message"]["input"]["summary"],
        "first input"
    );
    assert_eq!(
        duplicate.data["inbox_message"]["metadata"],
        json!(["caller metadata"])
    );
}

#[tokio::test]
async fn submission_identity_is_scoped_to_one_session() {
    let (_root, state, _channel) = connected_inbox().await;
    let inbox = state.inbox_commands();
    sqlx::query("UPDATE sessions SET state='busy' WHERE session_id='session'")
        .execute(&state.db())
        .await
        .unwrap();
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

    let error = inbox
        .submit_message_once("message", "other", request("input"))
        .await
        .unwrap_err();

    assert!(matches!(error, Error::StateConflict(_)));
}
