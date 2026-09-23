use super::*;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};

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
                .insert_message(
                    &format!("{client}-{suffix}"),
                    client,
                    "after_idle",
                    "input",
                    "{}",
                    None,
                )
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
        assert_eq!(first.state, "failed");
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
            1
        );
        repository
            .mark_dispatched(&format!("{client}-two"), None)
            .await
            .unwrap();
        repository
            .insert_message(
                &format!("{client}-three"),
                client,
                "after_idle",
                "input",
                "{}",
                None,
            )
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
