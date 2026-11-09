use super::*;
use crate::runtime::SubscriptionState;

fn unloaded() -> Value {
    json!({"method":"thread/status/changed","params":{"threadId":"thread","status":{"type":"notLoaded"}}})
}

async fn wait_unobserved(fixture: &Fixture) {
    let runtime = CodexRuntime::existing(fixture.root.path()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while runtime.subscription(&fixture.session).await.is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn quiet_connections_and_missing_subscriptions_do_not_trigger_recovery() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    let methods = fixture.server_state.methods.lock().await.clone();
    tokio::time::sleep(Duration::from_millis(2200)).await;
    assert_eq!(*fixture.server_state.methods.lock().await, methods);

    let runtime = CodexRuntime::existing(fixture.root.path()).await.unwrap();
    runtime.clear_subscription(&fixture.session).await;
    tokio::time::sleep(Duration::from_millis(2200)).await;
    assert_eq!(*fixture.server_state.methods.lock().await, methods);
    assert_eq!(runtime.subscription(&fixture.session).await, None);
    fixture.shutdown().await;
}

#[tokio::test]
async fn unload_survives_reconnect_until_explicit_exit_and_resume() {
    let mut fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    fixture.notify(unloaded()).await;
    wait_unobserved(&fixture).await;
    assert!(
        fixture
            .app
            .live_output()
            .subscribe_session(&fixture.session)
            .initial_snapshot
            .is_none()
    );
    let before = fixture
        .server_state
        .methods
        .lock()
        .await
        .iter()
        .filter(|m| *m == "thread/resume")
        .count();
    let runtime = CodexRuntime::install_for_test(fixture.root.path(), fixture.socket.clone())
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(2200)).await;
    assert_eq!(runtime.subscription(&fixture.session).await, None);
    assert_eq!(
        fixture
            .server_state
            .methods
            .lock()
            .await
            .iter()
            .filter(|m| *m == "thread/resume")
            .count(),
        before
    );
    let state: String = sqlx::query_scalar("SELECT state FROM turns WHERE turn_id=?")
        .bind(fixture.turn_id().await)
        .fetch_one(&fixture.app.db())
        .await
        .unwrap();
    assert_eq!(state, "running");

    // Restart observation with an empty runtime registry, as on pontiad startup.
    fixture.shutdown.send(true).unwrap();
    fixture.observer.await.unwrap();
    CodexRuntime::shutdown(fixture.root.path()).await;
    let runtime = CodexRuntime::install_for_test(fixture.root.path(), fixture.socket.clone())
        .await
        .unwrap();
    fixture.shutdown.send_replace(false);
    fixture.observer = tokio::spawn(
        CodexObserver::new(&fixture.app, fixture.root.path().into())
            .run(fixture.shutdown.subscribe()),
    );
    tokio::time::sleep(Duration::from_millis(2200)).await;
    assert_eq!(runtime.subscription(&fixture.session).await, None);
    assert_eq!(
        fixture
            .server_state
            .methods
            .lock()
            .await
            .iter()
            .filter(|m| *m == "thread/resume")
            .count(),
        before
    );

    fixture
        .app
        .session_commands()
        .terminate_session(&fixture.session)
        .await
        .unwrap();
    fixture
        .app
        .session_commands()
        .resume_session(&fixture.session, fixture.root.path())
        .await
        .unwrap();
    assert_eq!(
        runtime.subscription(&fixture.session).await,
        Some(SubscriptionState::Available)
    );
    fixture.wait_snapshot(|_| true).await;
    fixture.shutdown().await;
}

#[tokio::test]
async fn lost_unload_notification_during_snapshot_does_not_reload_or_finish_the_turn() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    let mut notifications = vec![unloaded()];
    notifications.extend((0..4200).map(|_| delta("thread", "native-turn", "item", json!("stale"))));
    *fixture.server_state.before_snapshot.lock().await = notifications;
    fixture.notify(json!({"method":"turn/started","params":{"threadId":"thread","turn":{"id":"native-turn"}}})).await;
    wait_unobserved(&fixture).await;
    tokio::time::sleep(Duration::from_millis(2200)).await;
    assert_eq!(
        fixture
            .server_state
            .methods
            .lock()
            .await
            .iter()
            .filter(|m| *m == "thread/resume")
            .count(),
        1
    );
    assert!(
        fixture
            .app
            .live_output()
            .subscribe_session(&fixture.session)
            .initial_snapshot
            .is_none()
    );
    let state: String = sqlx::query_scalar("SELECT state FROM turns WHERE turn_id=?")
        .bind(fixture.turn_id().await)
        .fetch_one(&fixture.app.db())
        .await
        .unwrap();
    assert_eq!(state, "running");
    fixture.shutdown().await;
}

#[tokio::test]
async fn reconnect_recovers_offline_completion_without_a_dashboard_subscriber() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    let mut completed = turn(json!([]));
    completed["status"] = json!("completed");
    *fixture.server_state.turns.lock().await = vec![completed];
    fixture
        .server_state
        .connections
        .lock()
        .await
        .last()
        .unwrap()
        .send(Command::Disconnect)
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let state: String = sqlx::query_scalar("SELECT state FROM turns WHERE turn_id=?")
                .bind(fixture.turn_id().await)
                .fetch_one(&fixture.app.db())
                .await
                .unwrap();
            if state == "completed" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    fixture.shutdown().await;
}

#[tokio::test]
async fn failed_reconnect_does_not_claim_available_or_retry_on_a_timer() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    fixture.server_state.rpc_errors.lock().await.insert(
        "thread/resume".into(),
        json!({"code":-32000,"message":"unavailable"}),
    );
    let runtime = CodexRuntime::install_for_test(fixture.root.path(), fixture.socket.clone())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if fixture
                .server_state
                .methods
                .lock()
                .await
                .iter()
                .filter(|m| *m == "thread/resume")
                .count()
                == 2
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(2200)).await;
    assert_eq!(runtime.subscription(&fixture.session).await, None);
    assert_eq!(
        fixture
            .server_state
            .methods
            .lock()
            .await
            .iter()
            .filter(|m| *m == "thread/resume")
            .count(),
        2
    );
    let state: String = sqlx::query_scalar("SELECT state FROM turns WHERE turn_id=?")
        .bind(fixture.turn_id().await)
        .fetch_one(&fixture.app.db())
        .await
        .unwrap();
    assert_eq!(state, "running");
    fixture.shutdown().await;
}

#[tokio::test]
async fn exit_serializes_with_snapshot_recovery_and_cannot_be_overwritten() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    *fixture.server_state.snapshot_gate.lock().await = Some((entered, released));
    fixture.notify(json!({"method":"turn/started","params":{"threadId":"thread","turn":{"id":"native-turn"}}})).await;
    tokio::time::timeout(Duration::from_secs(3), waiting)
        .await
        .unwrap()
        .unwrap();
    let app = fixture.app.clone();
    let session = fixture.session.clone();
    let (started, starting) = tokio::sync::oneshot::channel();
    let mut exit = tokio::spawn(async move {
        let commands = app.session_commands();
        let exit = commands.terminate_session(&session);
        tokio::pin!(exit);
        let mut started = Some(started);
        std::future::poll_fn(|cx| {
            let result = std::future::Future::poll(exit.as_mut(), cx);
            if let Some(started) = started.take() {
                let _ = started.send(());
            }
            result
        })
        .await
    });
    starting.await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut exit)
            .await
            .is_err(),
        "exit must wait for in-flight recovery"
    );
    release.send(()).unwrap();
    exit.await.unwrap().unwrap();
    wait_unobserved(&fixture).await;
    assert_eq!(
        fixture
            .app
            .queries()
            .get_session(&fixture.session)
            .await
            .unwrap()
            .unwrap()
            .state,
        "exited"
    );
    assert!(
        fixture
            .app
            .live_output()
            .subscribe_session(&fixture.session)
            .initial_snapshot
            .is_none()
    );
    assert_eq!(
        fixture
            .server_state
            .methods
            .lock()
            .await
            .iter()
            .filter(|m| *m == "thread/resume")
            .count(),
        1
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn lag_recovers_completion_before_unload_without_reloading() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    let mut completed = turn(json!([]));
    completed["status"] = json!("completed");
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    *fixture.server_state.snapshot_gate.lock().await = Some((entered, released));
    let mut notifications = vec![
        json!({"method":"turn/completed","params":{"threadId":"thread","turn":completed}}),
        unloaded(),
    ];
    notifications.extend((0..4200).map(|_| delta("thread", "native-turn", "item", json!("stale"))));
    *fixture.server_state.before_snapshot.lock().await = notifications;
    fixture.notify(json!({"method":"turn/started","params":{"threadId":"thread","turn":{"id":"native-turn"}}})).await;
    waiting.await.unwrap();
    *fixture.server_state.turns.lock().await = vec![completed];
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let state: String = sqlx::query_scalar("SELECT state FROM turns WHERE turn_id=?")
                .bind(fixture.turn_id().await)
                .fetch_one(&fixture.app.db())
                .await
                .unwrap();
            if state == "completed" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    wait_unobserved(&fixture).await;
    assert_eq!(
        fixture
            .server_state
            .methods
            .lock()
            .await
            .iter()
            .filter(|m| *m == "thread/resume")
            .count(),
        1
    );
    assert!(
        fixture
            .app
            .live_output()
            .subscribe_session(&fixture.session)
            .initial_snapshot
            .is_none()
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn exit_and_resume_while_notifications_wait_recovers_existing_output() {
    let fixture = Fixture::new(
        turn(json!([{"type":"agentMessage","id":"item","text":"existing"}])),
        Vec::new(),
    )
    .await;
    let runtime = CodexRuntime::existing(fixture.root.path()).await.unwrap();
    let connection = runtime.connection().await.unwrap();
    let associations = connection.interactions.lock().await;
    fixture.notify(json!({"id":"pending","method":"item/commandExecution/requestApproval","params":{"threadId":"thread","command":"echo hello"}})).await;
    tokio::time::timeout(Duration::from_secs(3), async {
        while connection.pending_server_requests().await.is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    fixture
        .app
        .session_commands()
        .terminate_session(&fixture.session)
        .await
        .unwrap();
    fixture
        .app
        .session_commands()
        .resume_session(&fixture.session, fixture.root.path())
        .await
        .unwrap();
    drop(associations);
    fixture
        .wait_snapshot(|snapshot| snapshot.items == vec![text("item", "existing")])
        .await;
    fixture.shutdown().await;
}

#[tokio::test]
async fn unload_during_reconnect_retains_real_terminal_history() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    let mut completed = turn(json!([]));
    completed["status"] = json!("completed");
    *fixture.server_state.turns.lock().await = vec![completed];
    *fixture.server_state.before_snapshot.lock().await = vec![unloaded()];
    let runtime = CodexRuntime::install_for_test(fixture.root.path(), fixture.socket.clone())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let state: String = sqlx::query_scalar("SELECT state FROM turns WHERE turn_id=?")
                .bind(fixture.turn_id().await)
                .fetch_one(&fixture.app.db())
                .await
                .unwrap();
            if state == "completed" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    wait_unobserved(&fixture).await;
    assert_eq!(runtime.subscription(&fixture.session).await, None);
    assert!(
        fixture
            .app
            .live_output()
            .subscribe_session(&fixture.session)
            .initial_snapshot
            .is_none()
    );
    fixture.shutdown().await;
}
