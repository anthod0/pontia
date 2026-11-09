use super::*;
use pontia_core::Error;
use std::sync::atomic::Ordering;

impl Fixture {
    async fn wait_method(&self, method: &str) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if self
                    .server_state
                    .methods
                    .lock()
                    .await
                    .iter()
                    .any(|seen| seen == method)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }

    async fn session_state(&self) -> String {
        self.app
            .queries()
            .get_session(&self.session)
            .await
            .unwrap()
            .unwrap()
            .state
    }

    async fn turn_state(&self) -> String {
        sqlx::query_scalar("SELECT state FROM turns WHERE turn_id=?")
            .bind(self.turn_id().await)
            .fetch_one(&self.app.db())
            .await
            .unwrap()
    }

    async fn reject_rpc(&self, method: &str) {
        self.server_state
            .rpc_errors
            .lock()
            .await
            .insert(method.into(), json!({"code":-32600,"message":"rejected"}));
    }

    async fn complete_native(&self, status: &str) {
        let turn = {
            let mut turns = self.server_state.turns.lock().await;
            turns[0]["status"] = json!(status);
            turns[0].clone()
        };
        self.notify(json!({"method":"turn/completed","params":{"threadId":"thread","turn":turn}}))
            .await;
    }
}

#[tokio::test]
async fn exit_waits_for_native_terminal_and_preserves_natural_completion() {
    let f = Fixture::new(turn(json!([])), vec![]).await;
    f.server_state.hold_interrupt.store(true, Ordering::SeqCst);
    let commands = f.app.session_commands();
    let session = f.session.clone();
    let exit = tokio::spawn(async move { commands.terminate_session(&session).await });
    f.wait_method("turn/interrupt").await;
    assert_eq!(f.session_state().await, "busy");
    assert!(!exit.is_finished());
    assert!(
        !f.server_state
            .methods
            .lock()
            .await
            .contains(&"thread/archive".into())
    );
    f.complete_native("completed").await;
    exit.await.unwrap().unwrap();
    assert_eq!(f.turn_state().await, "completed");
    assert_eq!(f.session_state().await, "exited");
    assert!(f.server_state.unloaded.load(Ordering::SeqCst));
    assert!(
        !f.server_state
            .methods
            .lock()
            .await
            .contains(&"thread/unsubscribe".into())
    );
    f.shutdown().await;
}

#[tokio::test]
async fn exit_records_interruption_and_preserves_identity_on_resume() {
    let f = Fixture::new(turn(json!([])), vec![]).await;
    let original = f.turn_id().await;
    f.app
        .session_commands()
        .terminate_session(&f.session)
        .await
        .unwrap();
    assert_eq!(f.turn_state().await, "interrupted");
    assert_eq!(f.session_state().await, "exited");
    f.app
        .session_commands()
        .resume_session(&f.session, f.root.path())
        .await
        .unwrap();
    assert_eq!(f.session_state().await, "idle");
    assert_eq!(f.turn_id().await, original);
    assert_eq!(
        AgentBindingService::new(f.app.db())
            .binding_for_session(&f.session)
            .await
            .unwrap()
            .unwrap()
            .client_session_key,
        "thread"
    );
    f.shutdown().await;
}

#[tokio::test]
async fn rejected_interrupt_preserves_observation_and_does_not_archive() {
    let f = Fixture::new(turn(json!([])), vec![]).await;
    f.reject_rpc("turn/interrupt").await;
    assert!(matches!(
        f.app.session_commands().terminate_session(&f.session).await,
        Err(Error::StateConflict(_))
    ));
    assert_eq!(f.session_state().await, "busy");
    assert!(
        !f.server_state
            .methods
            .lock()
            .await
            .contains(&"thread/archive".into())
    );
    f.complete_native("completed").await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while f.turn_state().await != "completed" {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(f.session_state().await, "idle");
    f.shutdown().await;
}

#[tokio::test]
async fn replacement_turn_prevents_archive_and_is_not_interrupted_again() {
    let f = Fixture::new(turn(json!([])), vec![]).await;
    let mut completed = turn(json!([]));
    completed["status"] = json!("completed");
    let mut replacement = turn(json!([]));
    replacement["id"] = json!("replacement");
    *f.server_state.after_interrupt.lock().await = Some(vec![completed, replacement]);
    assert!(matches!(
        f.app.session_commands().terminate_session(&f.session).await,
        Err(Error::ControlUnknown(_))
    ));
    assert_eq!(f.turn_state().await, "completed");
    assert_eq!(f.session_state().await, "busy");
    let methods = f.server_state.methods.lock().await;
    assert!(!methods.contains(&"thread/archive".into()));
    assert_eq!(methods.iter().filter(|m| *m == "turn/interrupt").count(), 1);
    drop(methods);
    f.shutdown().await;
}

#[tokio::test]
async fn missing_terminal_result_times_out_without_unloading_or_losing_observation() {
    let f = Fixture::new(turn(json!([])), vec![]).await;
    f.server_state.hold_interrupt.store(true, Ordering::SeqCst);
    assert!(matches!(
        f.app.session_commands().terminate_session(&f.session).await,
        Err(Error::ControlUnknown(_))
    ));
    assert_eq!(f.session_state().await, "busy");
    assert!(
        !f.server_state
            .methods
            .lock()
            .await
            .contains(&"thread/archive".into())
    );
    f.complete_native("interrupted").await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while f.turn_state().await != "interrupted" {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    f.shutdown().await;
}

#[tokio::test]
async fn archive_receipt_waits_for_actual_unload_before_restoring_history() {
    let f = Fixture::new(turn(json!([])), vec![]).await;
    f.server_state.hold_unload.store(true, Ordering::SeqCst);
    let commands = f.app.session_commands();
    let session = f.session.clone();
    let exit = tokio::spawn(async move { commands.terminate_session(&session).await });
    f.wait_method("thread/read").await;
    assert!(!exit.is_finished());
    assert_eq!(f.session_state().await, "interrupted");
    assert!(
        !f.server_state
            .methods
            .lock()
            .await
            .contains(&"thread/unarchive".into())
    );
    f.server_state.unloaded.store(true, Ordering::SeqCst);
    f.notify(json!({"method":"thread/status/changed","params":{"threadId":"thread","status":{"type":"notLoaded"}}})).await;
    exit.await.unwrap().unwrap();
    assert_eq!(f.session_state().await, "exited");
    f.shutdown().await;
}

#[tokio::test]
async fn unconfirmed_unload_times_out_without_unarchiving() {
    let f = Fixture::new(turn(json!([])), vec![]).await;
    f.server_state.hold_unload.store(true, Ordering::SeqCst);
    assert!(matches!(
        f.app.session_commands().terminate_session(&f.session).await,
        Err(Error::ControlUnknown(_))
    ));
    assert_eq!(f.session_state().await, "interrupted");
    assert_eq!(f.turn_state().await, "interrupted");
    assert!(
        !f.server_state
            .methods
            .lock()
            .await
            .contains(&"thread/unarchive".into())
    );
    f.shutdown().await;
}

#[tokio::test]
async fn archive_and_unarchive_rejections_preserve_confirmed_turn_result() {
    for method in ["thread/archive", "thread/unarchive"] {
        let f = Fixture::new(turn(json!([])), vec![]).await;
        f.reject_rpc(method).await;
        assert!(matches!(
            f.app.session_commands().terminate_session(&f.session).await,
            Err(Error::StateConflict(_))
        ));
        assert_eq!(f.session_state().await, "interrupted");
        assert_eq!(f.turn_state().await, "interrupted");
        assert_eq!(
            f.server_state.unloaded.load(Ordering::SeqCst),
            method == "thread/unarchive"
        );
        let methods = f.server_state.methods.lock().await;
        assert_eq!(methods.iter().filter(|m| m.as_str() == method).count(), 1);
        if method == "thread/archive" {
            assert!(!methods.contains(&"thread/unarchive".into()));
        }
        drop(methods);
        f.shutdown().await;
    }
}

#[tokio::test]
async fn lost_receipts_do_not_claim_exit_or_reload_after_archive_attempt() {
    for method in ["turn/interrupt", "thread/archive", "thread/unarchive"] {
        let f = Fixture::new(turn(json!([])), vec![]).await;
        *f.server_state.disconnect_on.lock().await = Some(method.into());
        assert!(matches!(
            f.app.session_commands().terminate_session(&f.session).await,
            Err(Error::ControlUnknown(_))
        ));
        assert_ne!(f.session_state().await, "exited");
        if method == "turn/interrupt" {
            assert_eq!(f.turn_state().await, "running");
            assert!(
                !f.server_state
                    .methods
                    .lock()
                    .await
                    .contains(&"thread/archive".into())
            );
        } else {
            assert_eq!(f.turn_state().await, "interrupted");
            // A fresh observer and connection must honor persisted reload suppression.
            f.server_state.methods.lock().await.clear();
            f.notify_recovery().await;
            assert!(
                !f.server_state
                    .methods
                    .lock()
                    .await
                    .contains(&"thread/resume".into())
            );
        }
        f.shutdown().await;
    }
}

impl Fixture {
    async fn notify_recovery(&self) {
        let runtime = CodexRuntime::install_for_test(self.root.path(), self.socket.clone())
            .await
            .unwrap();
        let observer = CodexObserver::new(&self.app, self.root.path().into());
        let (shutdown, receiver) = watch::channel(false);
        let task = tokio::spawn(observer.run(receiver));
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if self
                    .server_state
                    .methods
                    .lock()
                    .await
                    .contains(&"thread/turns/list".into())
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert!(runtime.subscription(&self.session).await.is_none());
        shutdown.send(true).unwrap();
        task.await.unwrap();
    }
}

#[tokio::test]
async fn unexpected_unarchive_identity_or_loaded_status_cannot_confirm_exit() {
    for thread in [
        json!({"id":"other","status":{"type":"notLoaded"}}),
        json!({"id":"thread","status":{"type":"active"}}),
    ] {
        let f = Fixture::new(turn(json!([])), vec![]).await;
        *f.server_state.unarchive_thread.lock().await = Some(thread);
        assert!(matches!(
            f.app.session_commands().terminate_session(&f.session).await,
            Err(Error::ControlUnknown(_))
        ));
        assert_eq!(f.turn_state().await, "interrupted");
        assert_eq!(f.session_state().await, "interrupted");
        f.shutdown().await;
    }
}

#[tokio::test]
async fn activity_received_during_terminal_snapshot_prevents_archive_and_keeps_observing() {
    let f = Fixture::new(turn(json!([])), vec![]).await;
    let mut next = turn(json!([]));
    next["id"] = json!("next-turn");
    let started = json!({"method":"turn/started","params":{"threadId":"thread","turn":next}});
    f.server_state
        .after_interrupt_events
        .lock()
        .await
        .push(started.clone());
    assert!(matches!(
        f.app.session_commands().terminate_session(&f.session).await,
        Err(Error::ControlUnknown(_))
    ));
    assert!(
        !f.server_state
            .methods
            .lock()
            .await
            .contains(&"thread/archive".into())
    );
    assert_eq!(f.turn_state().await, "interrupted");
    f.server_state.turns.lock().await.push(next);
    f.notify(started).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while f.session_state().await != "busy" {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    f.shutdown().await;
}

#[tokio::test]
async fn activity_received_during_final_unload_snapshot_prevents_exit_confirmation() {
    let f = Fixture::new(turn(json!([])), vec![]).await;
    f.server_state.before_final_read.lock().await.push(json!({
        "method":"thread/started","params":{"thread":{"id":"thread","status":{"type":"active"}}}
    }));
    assert!(matches!(
        f.app.session_commands().terminate_session(&f.session).await,
        Err(Error::ControlUnknown(_))
    ));
    assert_ne!(f.session_state().await, "exited");
    assert_eq!(f.turn_state().await, "interrupted");
    f.shutdown().await;
}

#[tokio::test]
async fn terminal_idle_notification_does_not_invalidate_stop_confirmation() {
    let f = Fixture::new(turn(json!([])), vec![]).await;
    f.server_state
        .after_interrupt_events
        .lock()
        .await
        .push(json!({
            "method":"thread/status/changed","params":{"threadId":"thread","status":{"type":"idle"}}
        }));
    f.app
        .session_commands()
        .terminate_session(&f.session)
        .await
        .unwrap();
    assert_eq!(f.session_state().await, "exited");
    assert_eq!(f.turn_state().await, "interrupted");
    f.shutdown().await;
}
