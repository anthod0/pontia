use super::*;
use pontia_storage_sqlite::repositories::session_runtimes::{
    SessionRuntimeRecord, SqliteSessionRuntimeRepository,
};

impl Fixture {
    async fn runtimes(
        &self,
        session: &str,
        running: usize,
        exited: usize,
    ) -> Vec<SessionRuntimeRecord> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let records = SqliteSessionRuntimeRepository::new(self.app.db())
                    .list(session)
                    .await
                    .unwrap();
                if records
                    .iter()
                    .filter(|record| record.role == "codex_proxy" && record.state == "running")
                    .count()
                    == running
                    && records
                        .iter()
                        .filter(|record| record.role == "codex_proxy" && record.state == "exited")
                        .count()
                        == exited
                {
                    return records;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap()
    }

    async fn exit_events(&self, session: &str) -> i64 {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM events WHERE session_id=? AND event_type='session.exited'",
        )
        .bind(session)
        .fetch_one(&self.app.db())
        .await
        .unwrap()
    }

    async fn management_calls(&self, native: &str) -> Vec<String> {
        self.calls
            .lock()
            .await
            .iter()
            .filter(|(pontia, method, thread)| {
                *pontia
                    && thread == native
                    && matches!(
                        method.as_str(),
                        "thread/unsubscribe"
                            | "turn/interrupt"
                            | "thread/archive"
                            | "thread/unarchive"
                    )
            })
            .map(|(_, method, _)| method.clone())
            .collect()
    }
}

#[tokio::test]
async fn last_proxy_unsubscribes_management_without_stopping_native_work() {
    let f = Fixture::new().await;
    let mut first = f.client().await;
    let response = rpc(&mut first, "thread/fork", json!({})).await;
    let native = response["result"]["thread"]["id"].as_str().unwrap();
    let session = f.session(native, "idle").await;
    let mut second = f.client().await;
    rpc(&mut second, "thread/resume", json!({"threadId":native})).await;
    f.runtimes(&session, 2, 0).await;
    rpc(&mut first, "turn/start", json!({"threadId":native})).await;
    f.session(native, "busy").await;
    rpc(&mut first, "thread/unsubscribe", json!({"threadId":native})).await;
    f.runtimes(&session, 1, 1).await;
    assert!(f.management_calls(native).await.is_empty());
    assert_eq!(f.exit_events(&session).await, 0);
    rpc(
        &mut second,
        "thread/unsubscribe",
        json!({"threadId":native}),
    )
    .await;
    f.session(native, "exited").await;
    f.runtimes(&session, 0, 2).await;
    assert_eq!(f.management_calls(native).await, ["thread/unsubscribe"]);
    assert_eq!(
        f.threads.lock().await[native].turns[0]["status"],
        "inProgress"
    );
    let state: String = sqlx::query_scalar("SELECT state FROM turns WHERE session_id=?")
        .bind(&session)
        .fetch_one(&f.app.db())
        .await
        .unwrap();
    assert_eq!(state, "running");
    rpc(&mut first, "thread/unsubscribe", json!({"threadId":native})).await;
    first.close(None).await.unwrap();
    second.close(None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(Ok(_)) = first.next().await {}
        while let Some(Ok(_)) = second.next().await {}
    })
    .await
    .unwrap();
    // Relay completion guarantees both close observations precede this adoption.
    let mut barrier = f.client().await;
    let response = rpc(&mut barrier, "thread/fork", json!({})).await;
    let barrier_session = f
        .session(response["result"]["thread"]["id"].as_str().unwrap(), "idle")
        .await;
    f.runtimes(&barrier_session, 1, 0).await;
    assert_eq!(f.exit_events(&session).await, 1);
    assert_eq!(f.management_calls(native).await, ["thread/unsubscribe"]);
    assert_eq!(f.threads.lock().await[native].subscribers.len(), 0);
    f.stop().await;
}

#[tokio::test]
async fn repeated_resume_reuses_runtime_and_reconnect_gets_a_new_runtime() {
    let f = Fixture::new().await;
    let mut cli = f.client().await;
    let response = rpc(&mut cli, "thread/fork", json!({})).await;
    let native = response["result"]["thread"]["id"].as_str().unwrap();
    let session = f.session(native, "idle").await;
    let original = f.runtimes(&session, 1, 0).await.remove(0);
    rpc(&mut cli, "thread/resume", json!({"threadId":native})).await;
    rpc(&mut cli, "thread/unsubscribe", json!({"threadId":native})).await;
    f.session(native, "exited").await;
    rpc(&mut cli, "thread/resume", json!({"threadId":native})).await;
    f.session(native, "idle").await;
    let restored = f.runtimes(&session, 1, 0).await.remove(0);
    assert_eq!(original, restored);
    assert!(
        restored.start_command.is_none()
            && restored.tmux_socket_path.is_none()
            && restored.tmux_pane_id.is_none()
            && restored.process_fingerprint.is_none()
    );
    cli.close(None).await.unwrap();
    f.session(native, "exited").await;
    let mut other = f.client().await;
    rpc(&mut other, "thread/resume", json!({"threadId":native})).await;
    assert_eq!(f.session(native, "idle").await, session);
    let records = f.runtimes(&session, 1, 1).await;
    assert_ne!(
        records
            .iter()
            .find(|r| r.state == "running")
            .unwrap()
            .runtime_id,
        original.runtime_id
    );
    f.stop().await;
}

#[tokio::test]
async fn thread_switch_and_normal_close_end_only_the_connections_remaining_associations() {
    let f = Fixture::new().await;
    let mut cli = f.client().await;
    let first = rpc(&mut cli, "thread/start", json!({})).await;
    let a = first["result"]["thread"]["id"].as_str().unwrap();
    let session_a = f.session(a, "idle").await;
    let second = rpc(&mut cli, "thread/fork", json!({})).await;
    let b = second["result"]["thread"]["id"].as_str().unwrap();
    let session_b = f.session(b, "idle").await;
    f.runtimes(&session_a, 1, 0).await;
    f.runtimes(&session_b, 1, 0).await;
    rpc(&mut cli, "thread/unsubscribe", json!({"threadId":a})).await;
    f.session(a, "exited").await;
    f.runtimes(&session_a, 0, 1).await;
    f.runtimes(&session_b, 1, 0).await;
    assert!(f.management_calls(b).await.is_empty());
    cli.close(None).await.unwrap();
    f.session(b, "exited").await;
    f.runtimes(&session_b, 0, 1).await;
    assert_eq!(f.exit_events(&session_a).await, 1);
    assert_eq!(f.exit_events(&session_b).await, 1);
    f.stop().await;
}

#[tokio::test]
async fn close_waits_for_queued_registration_and_skips_unadopted_threads() {
    let f = Fixture::new().await;
    let mut cli = f.client().await;
    let response = rpc(&mut cli, "thread/fork", json!({})).await;
    let native = response["result"]["thread"]["id"].as_str().unwrap();
    let session = f.session(native, "idle").await;
    let runtime = CodexRuntime::existing(f.root.path()).await.unwrap();
    let guard = runtime.lock_session(&session).await;
    let mut second = f.client().await;
    rpc(&mut second, "thread/resume", json!({"threadId":native})).await;
    rpc(&mut second, "thread/start", json!({"ephemeral":true})).await;
    second.close(None).await.unwrap();
    drop(guard);
    f.runtimes(&session, 1, 1).await;
    cli.close(None).await.unwrap();
    f.session(native, "exited").await;
    f.runtimes(&session, 0, 2).await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_runtimes")
        .fetch_one(&f.app.db())
        .await
        .unwrap();
    assert_eq!(count, 2);
    f.stop().await;
}

#[tokio::test]
async fn archive_ends_all_proxy_runtimes_even_during_dashboard_exit() {
    for dashboard in [false, true] {
        let f = Fixture::new().await;
        let mut cli = f.client().await;
        let response = rpc(&mut cli, "thread/fork", json!({})).await;
        let native = response["result"]["thread"]["id"].as_str().unwrap();
        let session = f.session(native, "idle").await;
        let mut second = f.client().await;
        rpc(&mut second, "thread/resume", json!({"threadId":native})).await;
        f.runtimes(&session, 2, 0).await;
        if dashboard {
            f.app
                .session_commands()
                .terminate_session(&session)
                .await
                .unwrap();
        } else {
            rpc(&mut cli, "thread/archive", json!({"threadId":native})).await;
        }
        f.session(native, "exited").await;
        f.runtimes(&session, 0, 2).await;
        assert_eq!(f.exit_events(&session).await, 1);
        assert_eq!(
            f.management_calls(native).await,
            if dashboard {
                vec!["thread/archive", "thread/unarchive"]
            } else {
                vec!["thread/unsubscribe"]
            }
        );
        rpc(&mut cli, "thread/unarchive", json!({"threadId":native})).await;
        rpc(&mut cli, "thread/resume", json!({"threadId":native})).await;
        assert_eq!(f.session(native, "idle").await, session);
        f.runtimes(&session, 1, 1).await;
        f.stop().await;
    }
}

#[tokio::test]
async fn concurrent_departures_and_new_subscription_leave_the_new_association_managed() {
    let f = Fixture::new().await;
    let mut first = f.client().await;
    let response = rpc(&mut first, "thread/fork", json!({})).await;
    let native = response["result"]["thread"]["id"].as_str().unwrap();
    let session = f.session(native, "idle").await;
    let mut second = f.client().await;
    rpc(&mut second, "thread/resume", json!({"threadId":native})).await;
    f.runtimes(&session, 2, 0).await;
    let mut third = f.client().await;
    let runtime = CodexRuntime::existing(f.root.path()).await.unwrap();
    let guard = runtime.lock_session(&session).await;
    tokio::join!(
        rpc(&mut first, "thread/unsubscribe", json!({"threadId":native})),
        rpc(
            &mut second,
            "thread/unsubscribe",
            json!({"threadId":native})
        ),
        rpc(&mut third, "thread/resume", json!({"threadId":native})),
    );
    drop(guard);
    f.runtimes(&session, 1, 2).await;
    assert_eq!(f.session(native, "idle").await, session);
    rpc(&mut third, "turn/start", json!({"threadId":native})).await;
    f.session(native, "busy").await;
    assert_eq!(f.runtimes(&session, 1, 2).await.len(), 3);
    f.stop().await;
}

#[tokio::test]
async fn delayed_archive_notification_does_not_end_a_later_cli_resubscription() {
    let mut f = Fixture::with_observer(false).await;
    let mut cli = f.client().await;
    let response = rpc(&mut cli, "thread/fork", json!({})).await;
    let native = response["result"]["thread"]["id"].as_str().unwrap();
    let session = f.session(native, "idle").await;
    let original = f.runtimes(&session, 1, 0).await.remove(0);
    let runtime = CodexRuntime::existing(f.root.path()).await.unwrap();
    let connection = runtime.connection().await.unwrap();
    let mut events = connection.events.subscribe();
    rpc(&mut cli, "thread/archive", json!({"threadId":native})).await;
    let archived = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = events.recv().await.unwrap();
            if event["method"] == "thread/archived" {
                break event;
            }
        }
    })
    .await
    .unwrap();
    rpc(&mut cli, "thread/unarchive", json!({"threadId":native})).await;
    rpc(&mut cli, "thread/resume", json!({"threadId":native})).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while f.exit_events(&session).await == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(f.session(native, "idle").await, session);
    assert_eq!(f.runtimes(&session, 1, 0).await[0], original);

    let reads_before = f
        .calls
        .lock()
        .await
        .iter()
        .filter(|(_, method, _)| method == "thread/read")
        .count();
    let observer = CodexObserver::new(&f.app, f.root.path().into());
    f.tasks.spawn(observer.run(f.shutdown.subscribe()));
    tokio::time::timeout(Duration::from_secs(5), async {
        while f
            .calls
            .lock()
            .await
            .iter()
            .filter(|(_, method, _)| method == "thread/read")
            .count()
            == reads_before
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    connection.events.send(archived).unwrap();
    f.threads.lock().await[native].notify(json!({"method":"thread/name/updated","params":{"threadId":native,"threadName":"after archive replay"}}));
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let name: Option<String> =
                sqlx::query_scalar("SELECT title FROM sessions WHERE session_id=?")
                    .bind(&session)
                    .fetch_one(&f.app.db())
                    .await
                    .unwrap();
            if name.as_deref() == Some("after archive replay") {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(f.runtimes(&session, 1, 0).await[0], original);
    assert_eq!(f.session(native, "idle").await, session);
    assert_eq!(f.exit_events(&session).await, 1);
    assert_eq!(f.management_calls(native).await, ["thread/unsubscribe"]);
    f.stop().await;
}
