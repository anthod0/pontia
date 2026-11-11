use super::*;
use pontia_application::{ContextUsageCapability, ContextUsageView};

fn usage(used: Value, max: Value) -> Value {
    json!({"method":"thread/tokenUsage/updated","params":{
        "threadId":"thread","turnId":"native-turn",
        "tokenUsage":{
            "last":{"totalTokens":used,"inputTokens":30000,"cachedInputTokens":10000,
                "cacheWriteInputTokens":0,"outputTokens":2000,"reasoningOutputTokens":1000},
            "total":{"totalTokens":999999},"modelContextWindow":max
        }
    }})
}

impl Fixture {
    async fn context_usage(&self) -> Option<ContextUsageView> {
        self.app
            .queries()
            .get_session(&self.session)
            .await
            .unwrap()
            .unwrap()
            .context_usage
    }

    async fn wait_usage(&self, used: u64, max: u64) -> ContextUsageView {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(usage) = self.context_usage().await
                    && usage.used_tokens == Some(used)
                    && usage.max_tokens == Some(max)
                {
                    return usage;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap()
    }

    async fn flush_notifications(&self) {
        let before = self.wait_snapshot(|_| true).await.sequence;
        self.notify(delta("thread", "native-turn", "barrier", json!(".")))
            .await;
        self.wait_snapshot(|snapshot| snapshot.sequence > before)
            .await;
    }

    async fn usage_event_count(&self) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE session_id=? AND event_type='session.context_usage_updated'")
            .bind(&self.session).fetch_one(&self.app.db()).await.unwrap()
    }
}

#[tokio::test]
async fn context_usage_replaces_snapshots_and_clamps_only_ratio_and_remaining() {
    let fixture = Fixture::new(turn(json!([])), vec![]).await;
    let session = fixture
        .app
        .queries()
        .get_session(&fixture.session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        session.capabilities.context_usage,
        ContextUsageCapability::Estimated
    );
    assert_eq!(session.context_usage, None);

    for (used, max, ratio, remaining) in [
        (32000, 128000, 0.25, 96000),
        (64000, 128000, 0.5, 64000),
        (16000, 128000, 0.125, 112000),
        (16000, 64000, 0.25, 48000),
        (80000, 64000, 1.0, 0),
        (0, 64000, 0.0, 64000),
    ] {
        fixture.notify(usage(json!(used), json!(max))).await;
        let observed = fixture.wait_usage(used, max).await;
        assert_eq!(observed.usage_ratio, Some(ratio));
        assert_eq!(observed.remaining_tokens, Some(remaining));
        assert_eq!(observed.confidence, "estimated");
    }
    assert_eq!(fixture.usage_event_count().await, 6);
    assert_eq!(fixture.session_state().await, "busy");
    fixture.shutdown().await;
}

#[tokio::test]
async fn incomplete_or_invalid_usage_preserves_the_entire_last_observation() {
    let fixture = Fixture::new(turn(json!([])), vec![]).await;
    for baseline in [None, Some((32000, 128000))] {
        if let Some((used, max)) = baseline {
            fixture.notify(usage(json!(used), json!(max))).await;
            fixture.wait_usage(used, max).await;
        }
        let before = fixture.context_usage().await;
        for (used, max) in [
            (json!(64000), Value::Null),
            (json!(64000), json!(0)),
            (json!(64000), json!(-1)),
            (json!(64000), json!(128000.5)),
            (json!(64000), json!("128000")),
            (Value::Null, json!(256000)),
            (json!(-1), json!(256000)),
            (json!(0.5), json!(256000)),
            (json!("64000"), json!(256000)),
        ] {
            fixture.notify(usage(used, max)).await;
        }
        let mut missing = usage(json!(64000), json!(256000));
        missing["params"]["tokenUsage"]
            .as_object_mut()
            .unwrap()
            .remove("last");
        fixture.notify(missing).await;
        let mut missing = usage(json!(64000), json!(256000));
        missing["params"]["tokenUsage"]
            .as_object_mut()
            .unwrap()
            .remove("modelContextWindow");
        fixture.notify(missing).await;
        fixture.flush_notifications().await;
        assert_eq!(fixture.context_usage().await, before);
    }
    assert_eq!(fixture.usage_event_count().await, 1);
    fixture.shutdown().await;
}

#[tokio::test]
async fn other_threads_and_compaction_completion_do_not_change_usage() {
    let fixture = Fixture::new(turn(json!([])), vec![]).await;
    fixture.notify(usage(json!(32000), json!(128000))).await;
    let before = fixture.wait_usage(32000, 128000).await;
    let mut other = usage(json!(64000), json!(256000));
    other["params"]["threadId"] = json!("other-thread");
    fixture.notify(other).await;
    fixture
        .notify(json!({"method":"item/completed","params":{
            "threadId":"thread","turnId":"native-turn",
            "item":{"type":"contextCompaction","id":"compaction"}
        }}))
        .await;
    fixture.flush_notifications().await;
    assert_eq!(fixture.context_usage().await, Some(before));
    assert_eq!(fixture.usage_event_count().await, 1);
    fixture.shutdown().await;
}

#[tokio::test]
async fn recovery_accepts_usage_for_a_completed_turn_without_new_execution() {
    let mut fixture = Fixture::new(turn(json!([])), vec![]).await;
    fixture.notify(usage(json!(32000), json!(128000))).await;
    let before = fixture.wait_usage(32000, 128000).await;
    fixture.complete_native("completed").await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while fixture.session_state().await != "idle" {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    fixture.shutdown.send(true).unwrap();
    fixture.observer.await.unwrap();
    CodexRuntime::shutdown(fixture.root.path()).await;
    assert_eq!(
        fixture
            .app
            .queries()
            .get_session(&fixture.session)
            .await
            .unwrap()
            .unwrap()
            .context_usage,
        Some(before)
    );
    CodexRuntime::install_for_test(fixture.root.path(), fixture.socket.clone())
        .await
        .unwrap();
    *fixture.server_state.before_snapshot.lock().await = vec![usage(json!(16000), json!(128000))];
    fixture.shutdown.send_replace(false);
    fixture.observer = tokio::spawn(
        CodexObserver::new(&fixture.app, fixture.root.path().into())
            .run(fixture.shutdown.subscribe()),
    );

    fixture.wait_usage(16000, 128000).await;
    assert_eq!(fixture.session_state().await, "idle");
    assert_eq!(fixture.turn_state().await, "completed");
    let turns: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns WHERE session_id=?")
        .bind(&fixture.session)
        .fetch_one(&fixture.app.db())
        .await
        .unwrap();
    assert_eq!(turns, 1);
    assert_eq!(fixture.usage_event_count().await, 2);
    fixture.shutdown().await;
}

#[tokio::test]
async fn replaced_connection_cannot_publish_queued_usage() {
    let fixture = Fixture::new(turn(json!([])), vec![]).await;
    fixture.notify(usage(json!(32000), json!(128000))).await;
    let before = fixture.wait_usage(32000, 128000).await;
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    *fixture.server_state.snapshot_gate.lock().await = Some((entered, released));
    *fixture.server_state.before_snapshot.lock().await = vec![usage(json!(64000), json!(128000))];
    fixture.notify(json!({"method":"turn/started","params":{"threadId":"thread","turn":{"id":"native-turn"}}})).await;
    tokio::time::timeout(Duration::from_secs(5), waiting)
        .await
        .unwrap()
        .unwrap();
    CodexRuntime::install_for_test(fixture.root.path(), fixture.socket.clone())
        .await
        .unwrap();
    let _ = release.send(());
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let runtime = CodexRuntime::existing(fixture.root.path()).await.unwrap();
            if runtime.subscription(&fixture.session).await
                == Some(crate::runtime::SubscriptionState::Available)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    fixture.flush_notifications().await;
    assert_eq!(fixture.context_usage().await, Some(before));
    assert_eq!(fixture.usage_event_count().await, 1);
    fixture.shutdown().await;
}
