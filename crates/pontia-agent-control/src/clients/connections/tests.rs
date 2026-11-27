use super::*;
use crate::clients::testing::{Channel, channel};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};

async fn fixture() -> (SqlitePool, tempfile::TempDir, ClientControlService) {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    sqlx::query("INSERT INTO sessions(session_id,client_type,state) VALUES ('session','test-channel','idle')").execute(&pool).await.unwrap();
    sqlx::query(r#"INSERT INTO session_runtimes(session_id, runtime_id, role, state, created_at) VALUES ('session', 'original', 'generic_control', 'running', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))"#).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO agent_bindings(id,session_id,client_type,launch_cwd,client_session_key,metadata) VALUES ('binding','session','test-channel','/unused','native','{}')").execute(&pool).await.unwrap();
    let control = ClientControlService::new(pool.clone(), root.path().into())
        .with_clients(crate::clients::testing::clients());
    (pool, root, control)
}

struct StartOnlyChannel;

impl crate::ClientControlChannel for StartOnlyChannel {
    fn available(&self) -> bool {
        true
    }

    fn invalidate(&self) {}

    fn shutdown(&self) -> crate::ClientControlOperation<'_> {
        Box::pin(async { Ok(()) })
    }

    fn ping(&self) -> crate::ClientControlOperation<'_> {
        Box::pin(async { Ok(()) })
    }

    fn replay<'a>(&'a self, _: &'a str) -> crate::ClientControlOperation<'a> {
        Box::pin(async { Ok(()) })
    }

    fn start_turn<'a>(&'a self, _: &'a crate::InputPayload) -> crate::DeliveryOperation<'a> {
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn default_delivery_primitives_preserve_unsupported_classification() {
    let (_, _root, control) = fixture().await;
    control
        .attach(
            "test-channel",
            "session",
            "original",
            "native",
            Arc::new(StartOnlyChannel),
        )
        .await
        .unwrap();

    let payload = crate::InputPayload::from("steer");
    assert!(matches!(
        control.steer("session", "original", &payload).await,
        Err(crate::DeliveryError::Unsupported(_))
    ));
    assert!(matches!(
        control.interrupt("session", "original").await,
        Err(crate::DeliveryError::Unsupported(_))
    ));
}

#[tokio::test]
async fn identity_conflicts_cannot_replace_a_current_channel() {
    let (_, _root, control) = fixture().await;
    let current = channel();
    control
        .attach(
            "test-channel",
            "session",
            "original",
            "native",
            current.clone(),
        )
        .await
        .unwrap();
    for (client, runtime, native) in [
        ("another-client", "original", "native"),
        ("test-channel", "stale", "native"),
        ("test-channel", "original", "another-native"),
        ("test-channel", "original", "native"),
    ] {
        assert!(
            control
                .attach(client, "session", runtime, native, channel())
                .await
                .is_err()
        );
    }
    let payload = crate::InputPayload::from("one input");
    control
        .start_turn("session", "original", &payload)
        .await
        .unwrap();
    assert_eq!(*current.input.lock().unwrap(), ["one input"]);
    assert!(current.available());
}

#[tokio::test]
async fn reasoning_effort_control_uses_the_current_connection() {
    let (_, _root, control) = fixture().await;
    let current = channel();
    control
        .attach(
            "test-channel",
            "session",
            "original",
            "native",
            current.clone(),
        )
        .await
        .unwrap();
    assert!(
        control
            .reasoning_effort_control_available("session")
            .await
            .unwrap()
    );

    control
        .set_reasoning_effort("session", "original", "high")
        .await
        .unwrap();

    assert_eq!(
        current.reasoning_efforts.lock().unwrap().as_slice(),
        ["high"]
    );
}

#[tokio::test]
async fn replacement_during_submission_returns_unknown_without_replaying_input() {
    let (pool, _root, control) = fixture().await;
    let old = Arc::new(Channel {
        delayed: true,
        ..Default::default()
    });
    control
        .attach("test-channel", "session", "original", "native", old.clone())
        .await
        .unwrap();
    let submission = tokio::spawn({
        let control = control.clone();
        async move {
            let payload = crate::InputPayload::from("one input");
            control.start_turn("session", "original", &payload).await
        }
    });
    tokio::time::timeout(std::time::Duration::from_secs(2), old.started.notified())
        .await
        .unwrap();
    sqlx::query("UPDATE session_runtimes SET runtime_id='replacement' WHERE session_id='session'")
        .execute(&pool)
        .await
        .unwrap();
    let new = channel();
    control
        .attach(
            "test-channel",
            "session",
            "replacement",
            "native",
            new.clone(),
        )
        .await
        .unwrap();
    old.finish.notify_one();
    let error = tokio::time::timeout(std::time::Duration::from_secs(2), submission)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(matches!(error, crate::DeliveryError::Uncertain(_)));
    assert!(!old.available());
    assert_eq!(*old.input.lock().unwrap(), ["one input"]);
    assert!(new.input.lock().unwrap().is_empty());
    let state: String = sqlx::query_scalar("SELECT state FROM sessions WHERE session_id='session'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, "idle");
    let facts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(facts, 0);
}

#[tokio::test]
async fn retired_shutdown_receipt_stays_unknown_after_replacement_also_exits() {
    let (pool, _root, control) = fixture().await;
    let old = Arc::new(Channel {
        delayed: true,
        ..Default::default()
    });
    control
        .attach("test-channel", "session", "original", "native", old.clone())
        .await
        .unwrap();
    let shutdown = tokio::spawn({
        let control = control.clone();
        async move { control.shutdown("session", "original").await }
    });
    tokio::time::timeout(std::time::Duration::from_secs(2), old.started.notified())
        .await
        .unwrap();
    // The old connection's own exit removes availability before resume retires it.
    sqlx::query("UPDATE sessions SET state='exited' WHERE session_id='session'")
        .execute(&pool)
        .await
        .unwrap();
    control.refresh_session("session").await;
    {
        let _guard = control.lock_identity().await;
        control.retire_connection_locked("session").await;
        sqlx::query("UPDATE sessions SET state='starting' WHERE session_id='session'")
            .execute(&pool)
            .await
            .unwrap();
    }
    let replacement = channel();
    control
        .attach(
            "test-channel",
            "session",
            "original",
            "native",
            replacement.clone(),
        )
        .await
        .unwrap();
    sqlx::query("UPDATE sessions SET state='exited' WHERE session_id='session'")
        .execute(&pool)
        .await
        .unwrap();
    control.refresh_session("session").await;
    old.finish.notify_one();
    assert!(matches!(
        tokio::time::timeout(std::time::Duration::from_secs(2), shutdown)
            .await
            .unwrap()
            .unwrap(),
        Err(Error::ControlUnknown(_))
    ));
    assert!(!replacement.available());
    assert!(!control.available("session").await.unwrap());
}

#[tokio::test]
async fn launch_attachment_does_not_claim_a_native_conversation() {
    use crate::ipc::identity::BindingConfirmation;
    let (pool, _root, control) = fixture().await;
    sqlx::query("DELETE FROM agent_bindings")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE session_runtimes SET state='starting'")
        .execute(&pool)
        .await
        .unwrap();
    let current = channel();
    let guard = control.lock_identity().await;
    control
        .attach_binding_locked(
            &guard,
            "test-channel",
            "session",
            "original",
            BindingConfirmation::Launch,
            current.clone(),
        )
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_bindings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    sqlx::query("INSERT INTO agent_bindings(id,session_id,client_type,launch_cwd,client_session_key,metadata) VALUES ('binding','session','test-channel','/unused','native','{}')").execute(&pool).await.unwrap();
    control
        .attach_binding_locked(
            &guard,
            "test-channel",
            "session",
            "original",
            BindingConfirmation::Launch,
            current.clone(),
        )
        .await
        .unwrap();
    sqlx::query("UPDATE session_runtimes SET state='running'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        control
            .attach_binding_locked(
                &guard,
                "test-channel",
                "session",
                "original",
                BindingConfirmation::Launch,
                current.clone()
            )
            .await
            .is_err()
    );
    control
        .attach_locked(
            &guard,
            "test-channel",
            "session",
            "original",
            "native",
            current.clone(),
        )
        .await
        .unwrap();
    assert!(
        control
            .attach_locked(
                &guard,
                "test-channel",
                "session",
                "original",
                "other",
                current
            )
            .await
            .is_err()
    );
}
