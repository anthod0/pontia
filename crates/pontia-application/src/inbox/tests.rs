use super::*;
use crate::EventIngestService;
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
    let service = InboxCommandService::new(
        EventIngestService::new(pool.clone()).with_clients(crate::clients::testing::clients()),
    );
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
    let events =
        EventIngestService::new(pool.clone()).with_clients(crate::clients::testing::clients());
    let session = crate::SessionCommandService::new(events.clone(), root.path().into());
    for error in [
        session
            .ensure_current_runtime("session", "old")
            .await
            .unwrap_err(),
        session
            .request_exit("session", Some("old"))
            .await
            .unwrap_err(),
        crate::TurnCommandService::new(events.clone())
            .interrupt_turn_for_runtime("session", "turn", "old")
            .await
            .unwrap_err(),
    ] {
        assert!(matches!(error, Error::StateConflict(_)));
    }
    let error = crate::TurnCommandService::new(events)
        .dispatch_initial(
            &crate::runtime::control_target::ControlTarget {
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
