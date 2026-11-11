use super::*;
use crate::CodexService;
use pontia_agent_control::client_contract::native_history::HistorySource;
use pontia_agent_control::{TurnTimelineDirection, TurnTimelineService};

async fn page(
    f: &Fixture,
    source: Option<HistorySource>,
    anchor: Option<String>,
    limit: usize,
) -> pontia_agent_control::TurnTimelinePage {
    TurnTimelineService::new(f.app.event_ingest_service())
        .with_history_source(source)
        .page(
            f.session.clone(),
            TurnTimelineDirection::Backward,
            anchor,
            limit,
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn native_history_pages_exclude_reverted_execution_records_and_keep_the_session() {
    let f = Fixture::new(
        turn(json!([{"type":"agentMessage","id":"answer","text":"first"}])),
        vec![],
    )
    .await;
    let runtime = CodexRuntime::existing(f.root.path()).await.unwrap();
    let service = CodexService::new(f.app.event_ingest_service());
    let mut turns = f.server_state.turns.lock().await.clone();
    turns[0]["status"] = json!("completed");
    for (id, text) in [
        ("b:with:colon", "second"),
        ("c", "removed"),
        ("d", "also removed"),
    ] {
        turns.push(json!({"id":id,"status":"completed","items":[{"id":format!("u-{id}"),"type":"userMessage","content":[{"type":"text","text":text}]}]}));
    }
    service
        .reconcile_turns(&f.session, &runtime, &turns)
        .await
        .unwrap();
    let old_turns: Vec<(String, String)> =
        sqlx::query_as("SELECT turn_id,state FROM turns WHERE session_id=? ORDER BY turn_id")
            .bind(&f.session)
            .fetch_all(&f.app.db())
            .await
            .unwrap();
    sqlx::query("UPDATE turns SET head_cursor=NULL,tail_cursor=NULL WHERE turn_id=?")
        .bind(&old_turns[1].0)
        .execute(&f.app.db())
        .await
        .unwrap();
    *f.server_state.turns.lock().await = turns[..2].to_vec();
    f.server_state
        .history_page_size
        .store(1, std::sync::atomic::Ordering::SeqCst);
    f.server_state.history_requests.lock().await.clear();
    f.notify(json!({"method":"thread/status/changed","params":{"threadId":"thread","status":{"type":"notLoaded"}}})).await;
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let suspended: bool = sqlx::query_scalar("SELECT COALESCE(json_extract(metadata,'$.codex_observation_unloaded'),false) FROM sessions WHERE session_id=?").bind(&f.session).fetch_one(&f.app.db()).await.unwrap();
            if suspended { break; }
            tokio::task::yield_now().await;
        }
    }).await.unwrap();
    f.notify(json!({"method":"thread/status/changed","params":{"threadId":"thread","status":{"type":"idle"}}})).await;
    let mut changes = f.app.agent_events().subscribe_history();
    f.notify(json!({"method":"thread/reverted","params":{"threadId":"thread"}}))
        .await;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), changes.recv())
            .await
            .unwrap()
            .unwrap(),
        f.session
    );
    assert!(
        f.app
            .live_output()
            .subscribe_session(&f.session)
            .initial_snapshot
            .is_none()
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        while runtime.subscription(&f.session).await
            != Some(crate::runtime::SubscriptionState::Available)
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let suspended: bool = sqlx::query_scalar("SELECT COALESCE(json_extract(metadata,'$.codex_observation_unloaded'),false) FROM sessions WHERE session_id=?").bind(&f.session).fetch_one(&f.app.db()).await.unwrap();
    assert!(!suspended);
    f.server_state.history_requests.lock().await.clear();
    let first = page(&f, None, None, 1).await;
    assert_eq!(first.history_source, Some(HistorySource::Native));
    let requests = f.server_state.history_requests.lock().await.clone();
    assert!(!requests[0].as_object().unwrap().contains_key("cursor"));
    assert_eq!(requests[1]["cursor"], "opaque/page/1");
    assert_eq!(first.items.len(), 1);
    assert_eq!(first.items[0].item.content_preview, "second");
    let second = page(&f, first.history_source, first.next_turn_id, 1).await;
    assert!(second.next_turn_id.is_none());
    assert_eq!(second.items[1].item.content_preview, "first");
    let actual: Vec<(String, String)> =
        sqlx::query_as("SELECT turn_id,state FROM turns WHERE session_id=? ORDER BY turn_id")
            .bind(&f.session)
            .fetch_all(&f.app.db())
            .await
            .unwrap();
    assert_eq!(actual, old_turns);
    // Old v1 offsets are irrelevant to current native history, even beyond EOF.
    let binding = AgentBindingService::new(f.app.db())
        .binding_for_session(&f.session)
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE turns SET head_cursor=?,tail_cursor=? WHERE turn_id=?")
        .bind(json!({"version":1,"binding":binding.id,"thread":"thread","native_turn":"native-turn","offset":999999}).to_string())
        .bind(json!({"version":1,"binding":binding.id,"native_turn":"native-turn","offset":9999999}).to_string())
        .bind(&old_turns[0].0).execute(&f.app.db()).await.unwrap();
    assert_eq!(page(&f, None, None, 10).await.items.len(), 3);
    *f.server_state.turns.lock().await = vec![];
    let empty = page(&f, None, None, 1).await;
    assert!(empty.items.is_empty());
    assert!(empty.next_turn_id.is_none());
    f.shutdown().await;
}

#[tokio::test]
async fn history_query_errors_propagate_and_unloaded_threads_are_not_resumed() {
    let f = Fixture::new(
        turn(json!([{"type":"agentMessage","id":"answer","text":"native only"}])),
        vec![],
    )
    .await;
    let path = f.root.path().join("preview.jsonl");
    let records = [
        json!({"type":"session_meta","payload":{"id":"thread"}}),
        json!({"type":"turn_context","payload":{"turn_id":"native-turn"}}),
        json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"text":"file-only untruncated output"}]}}),
    ];
    std::fs::write(
        &path,
        records
            .iter()
            .map(|record| format!("{record}\n"))
            .collect::<String>(),
    )
    .unwrap();
    sqlx::query("UPDATE agent_bindings SET client_session_file=? WHERE session_id=?")
        .bind(path.to_str().unwrap())
        .bind(&f.session)
        .execute(&f.app.db())
        .await
        .unwrap();
    f.server_state
        .unloaded
        .store(true, std::sync::atomic::Ordering::SeqCst);
    f.server_state.methods.lock().await.clear();
    let current = page(&f, None, None, 20).await;
    assert_eq!(current.history_source, Some(HistorySource::Native));
    assert_eq!(current.items[1].item.content_preview, "native only");
    assert!(
        !f.server_state
            .methods
            .lock()
            .await
            .iter()
            .any(|method| method == "thread/resume")
    );
    f.server_state.rpc_errors.lock().await.insert(
        "thread/turns/list".into(),
        json!({"code":-32601,"message":"unsupported history query"}),
    );
    let result = TurnTimelineService::new(f.app.event_ingest_service())
        .page(f.session.clone(), TurnTimelineDirection::Backward, None, 20)
        .await;
    assert!(result.is_err());
    let preview = page(&f, Some(HistorySource::File), None, 20).await;
    assert_eq!(
        preview.items[0].item.content_preview,
        "file-only untruncated output"
    );
    f.server_state.rpc_errors.lock().await.clear();
    assert_eq!(
        page(&f, preview.history_source, None, 20).await.items[0]
            .item
            .content_preview,
        "file-only untruncated output"
    );
    assert_eq!(
        page(&f, None, None, 20).await.items[1].item.content_preview,
        "native only"
    );

    assert!(
        !f.server_state
            .methods
            .lock()
            .await
            .iter()
            .any(|method| method == "thread/resume")
    );
    f.shutdown().await;
}

#[tokio::test]
async fn a_later_unload_is_not_cleared_by_a_delayed_revert_notification() {
    let f = Fixture::new(turn(json!([])), vec![]).await;
    let runtime = CodexRuntime::existing(f.root.path()).await.unwrap();
    let connection = runtime.connection().await.unwrap();
    let operation = runtime.lock_session(&f.session).await;
    let mut changes = f.app.agent_events().subscribe_history();
    f.notify(json!({"method":"thread/reverted","params":{"threadId":"thread"}}))
        .await;
    f.notify(json!({"method":"thread/status/changed","params":{"threadId":"thread","status":{"type":"notLoaded"}}})).await;
    tokio::time::timeout(Duration::from_secs(3), async {
        while !connection
            .unloaded_threads
            .lock()
            .await
            .contains_key("thread")
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    drop(operation);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), changes.recv())
            .await
            .unwrap()
            .unwrap(),
        f.session
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        while runtime.subscription(&f.session).await.is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(
        connection
            .unloaded_threads
            .lock()
            .await
            .contains_key("thread")
    );
    f.shutdown().await;
}
