use super::*;

#[tokio::test]
async fn manual_compaction_survives_reattachment_and_replays_one_checkpoint() {
    let mut h = Harness::new(&[("ACP_COMPACT", "1")]).await;
    let session = h.create("grok").await.unwrap();
    let turn = h
        .app
        .turn_commands()
        .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    h.release("release-1");
    h.wait_turn(&session, &turn.turn_id, "completed").await;
    let receipt = h
        .app
        .session_commands()
        .compact_session(&session)
        .await
        .unwrap();
    assert!(
        h.app
            .session_commands()
            .compact_session(&session)
            .await
            .is_err()
    );
    h.stop_transport().await;
    h.release("release-compact");
    h.start_transport().await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while h.events(&session, "session.compaction_completed").await == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    h.wait_ready(&session, 2).await;
    let payload: String = sqlx::query_scalar("SELECT payload FROM events WHERE session_id=? AND event_type='session.compaction_completed'")
        .bind(&session).fetch_one(&h.app.db()).await.unwrap();
    let payload: Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(payload["operation_id"], receipt.operation_id);
    assert_eq!(payload["trigger"], "manual");
    assert_eq!(
        (
            payload["tokens_before"].as_u64(),
            payload["tokens_after"].as_u64()
        ),
        (Some(1024), Some(1024))
    );
    for _ in 0..2 {
        let page = h.history(&session).await.unwrap();
        assert_eq!(
            page.items
                .iter()
                .map(|i| i.item.kind.as_str())
                .collect::<Vec<_>>(),
            ["user", "assistant", "context_compaction"]
        );
        assert!(page.items[2].turn_id.is_none());
    }
    assert_eq!(h.events(&session, "session.compaction_completed").await, 1);
    assert_eq!(h.requests("session/prompt"), 1);
    h.release("reject-compact");
    let rejected = h
        .app
        .session_commands()
        .compact_session(&session)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while h.events(&session, "session.compaction_failed").await == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let operation: String = sqlx::query_scalar("SELECT json_extract(payload,'$.operation_id') FROM events WHERE session_id=? AND event_type='session.compaction_failed'")
        .bind(&session).fetch_one(&h.app.db()).await.unwrap();
    assert_eq!(operation, rejected.operation_id);
    h.release("remove-compact");
    h.history(&session).await.unwrap();
    assert!(
        h.app
            .session_commands()
            .compact_session(&session)
            .await
            .is_err()
    );
    assert_eq!(h.requests("_x.ai/compact_conversation"), 2);
    h.stop().await;
}
