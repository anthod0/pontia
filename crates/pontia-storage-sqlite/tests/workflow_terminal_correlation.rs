use pontia_storage_sqlite::{
    connect_sqlite,
    repositories::events::{EventInsertRecord, SqliteEventRepository},
    run_migrations,
};
use serde_json::{Value, json};

async fn test_pool(root: &std::path::Path) -> sqlx::SqlitePool {
    let pool = connect_sqlite(&format!("sqlite://{}", root.join("events.db").display()))
        .await
        .unwrap();
    run_migrations(&pool).await.unwrap();
    pool
}

async fn fact(
    pool: &sqlx::SqlitePool,
    id: &str,
    session: &str,
    turn: Option<&str>,
    source: &str,
    event_type: &str,
    payload: Value,
) {
    let mut tx = pool.begin().await.unwrap();
    SqliteEventRepository::insert_event_in_tx(
        &mut tx,
        EventInsertRecord {
            event_id: id.into(),
            session_id: session.into(),
            turn_id: turn.map(str::to_string),
            source: source.into(),
            client_type: "pi".into(),
            event_type: event_type.into(),
            occurred_at: "2026-09-06T00:00:00Z".into(),
            payload: payload.to_string(),
            timeline_boundary: None,
            turn_topology: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn turn_terminal_runtime_comes_from_its_first_confirmed_start_not_terminal_payload() {
    let root = tempfile::tempdir().unwrap();
    let pool = test_pool(root.path()).await;
    let events = SqliteEventRepository::new(pool.clone());
    for (id, session, turn, source, runtime) in [
        ("other_session", "other", "turn", "agent_adapter", "wrong"),
        (
            "other_turn",
            "session",
            "other_turn",
            "agent_adapter",
            "wrong",
        ),
        ("command", "session", "turn", "external_api", "wrong"),
        ("start", "session", "turn", "agent_adapter", "original"),
        (
            "repeated_start",
            "session",
            "turn",
            "agent_adapter",
            "replacement",
        ),
    ] {
        fact(
            &pool,
            id,
            session,
            Some(turn),
            source,
            "turn.started",
            json!({ "runtime_id": runtime }),
        )
        .await;
    }
    fact(
        &pool,
        "terminal",
        "session",
        Some("turn"),
        "agent_adapter",
        "turn.interrupted",
        json!({ "runtime_id": "replacement" }),
    )
    .await;
    let terminal = events
        .latest_workflow_terminal_event("session", Some("original"), Some("turn"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(terminal.event_id, "terminal");
    assert_eq!(terminal.runtime_id.as_deref(), Some("original"));
    assert!(
        events
            .latest_workflow_terminal_event("session", Some("replacement"), Some("turn"))
            .await
            .unwrap()
            .is_none()
    );
    let stored = events.list_turn_events("session", "turn").await.unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&stored.last().unwrap().payload).unwrap(),
        json!({ "runtime_id": "replacement" }),
        "correlation must not rewrite persisted Agent facts"
    );
}

#[tokio::test]
async fn unstarted_turn_and_later_start_do_not_supply_fenced_terminal_evidence() {
    let root = tempfile::tempdir().unwrap();
    let pool = test_pool(root.path()).await;
    let events = SqliteEventRepository::new(pool.clone());
    fact(
        &pool,
        "terminal",
        "session",
        Some("turn"),
        "agent_adapter",
        "turn.interrupted",
        json!({}),
    )
    .await;
    assert!(
        events
            .latest_workflow_terminal_event("session", Some("runtime"), Some("turn"))
            .await
            .unwrap()
            .is_none()
    );
    fact(
        &pool,
        "late_start",
        "session",
        Some("turn"),
        "agent_adapter",
        "turn.started",
        json!({ "runtime_id": "runtime" }),
    )
    .await;
    assert!(
        events
            .latest_workflow_terminal_event("session", Some("runtime"), Some("turn"))
            .await
            .unwrap()
            .is_none()
    );
    let terminal = events
        .latest_workflow_terminal_event("session", None, None)
        .await
        .unwrap()
        .unwrap();
    assert!(terminal.runtime_id.is_none());
}

#[tokio::test]
async fn runtime_and_turn_fences_are_applied_before_selecting_terminal_evidence() {
    let root = tempfile::tempdir().unwrap();
    let pool = test_pool(root.path()).await;
    let events = SqliteEventRepository::new(pool.clone());
    fact(
        &pool,
        "old_exit",
        "session",
        None,
        "runtime_manager",
        "session.exited",
        json!({ "runtime_id": "old_runtime" }),
    )
    .await;
    fact(
        &pool,
        "start",
        "session",
        Some("turn"),
        "agent_adapter",
        "turn.started",
        json!({ "runtime_id": "runtime" }),
    )
    .await;
    fact(
        &pool,
        "terminal",
        "session",
        Some("turn"),
        "agent_adapter",
        "turn.completed",
        json!({}),
    )
    .await;
    fact(
        &pool,
        "other_start",
        "session",
        Some("other_turn"),
        "agent_adapter",
        "turn.started",
        json!({ "runtime_id": "runtime" }),
    )
    .await;
    fact(
        &pool,
        "other_terminal",
        "session",
        Some("other_turn"),
        "agent_adapter",
        "turn.failed",
        json!({}),
    )
    .await;
    let terminal = events
        .latest_workflow_terminal_event("session", Some("runtime"), Some("turn"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        terminal.event_id, "terminal",
        "an unrelated exit or Turn cannot mask the matching terminal"
    );
    let exited = events
        .latest_workflow_terminal_event("session", Some("old_runtime"), Some("turn"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        exited.event_id, "old_exit",
        "Session exit retains its own Runtime fence"
    );
}

#[tokio::test]
async fn reused_runtime_does_not_correlate_historical_exit_or_previous_turn_to_new_start() {
    let root = tempfile::tempdir().unwrap();
    let pool = test_pool(root.path()).await;
    let events = SqliteEventRepository::new(pool.clone());
    fact(
        &pool,
        "start",
        "session",
        Some("old-turn"),
        "agent_client",
        "turn.started",
        json!({"runtime_id":"stable"}),
    )
    .await;
    fact(
        &pool,
        "exit",
        "session",
        None,
        "agent_client",
        "session.exited",
        json!({"runtime_id":"stable"}),
    )
    .await;
    assert_eq!(
        events
            .latest_workflow_terminal_event("session", Some("stable"), None)
            .await
            .unwrap()
            .unwrap()
            .event_id,
        "exit"
    );
    fact(
        &pool,
        "resume",
        "session",
        None,
        "external_api",
        "session.resuming",
        json!({"runtime_id":"stable"}),
    )
    .await;
    // A terminal for a historical Turn may still be recorded for its history.
    fact(
        &pool,
        "late-terminal",
        "session",
        Some("old-turn"),
        "agent_client",
        "turn.failed",
        json!({}),
    )
    .await;
    assert!(
        events
            .latest_workflow_terminal_event("session", Some("stable"), None)
            .await
            .unwrap()
            .is_none()
    );
    fact(
        &pool,
        "new-start",
        "session",
        Some("new-turn"),
        "agent_client",
        "turn.started",
        json!({"runtime_id":"stable"}),
    )
    .await;
    fact(
        &pool,
        "new-terminal",
        "session",
        Some("new-turn"),
        "agent_client",
        "turn.completed",
        json!({}),
    )
    .await;
    assert_eq!(
        events
            .latest_workflow_terminal_event("session", Some("stable"), None)
            .await
            .unwrap()
            .unwrap()
            .event_id,
        "new-terminal"
    );
}
