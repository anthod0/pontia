use pontia_agent_control::{ControlQueryService, DomainEventReader};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};

#[tokio::test]
async fn filtered_event_pages_advance_over_unrelated_facts_and_survive_reopening() {
    let root = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", root.path().join("events.db").display());
    let pool = connect_sqlite(&url).await.unwrap();
    run_migrations(&pool).await.unwrap();
    sqlx::raw_sql("INSERT INTO sessions(session_id,client_type,state,application_id,application_ref) VALUES ('a','generic','idle','app-a','one'),('b','generic','idle','app-b','one');
        INSERT INTO events(event_id,session_id,source,client_type,event_type,occurred_at) VALUES ('a1','a','external_api','generic','session.created','2026-01-01'),('b1','b','external_api','generic','session.created','2026-01-01'),('a2','a','agent_client','generic','session.ready','2026-01-01');")
        .execute(&pool).await.unwrap();
    let reader = DomainEventReader::new(pool.clone());
    let first = reader.read_after(0, Some("app-a"), 1).await.unwrap();
    assert_eq!(
        first
            .events
            .iter()
            .map(|e| e.event_id.as_str())
            .collect::<Vec<_>>(),
        ["a1"]
    );
    let second = reader
        .read_after(first.next_position, Some("app-a"), 1)
        .await
        .unwrap();
    assert_eq!(second.events[0].event_id, "a2");
    let empty = reader.read_after(0, Some("absent"), 1).await.unwrap();
    assert!(empty.events.is_empty());
    assert_eq!(empty.next_position, second.next_position);
    let cursor = second.next_position;
    drop(reader);
    pool.close().await;
    let reopened = connect_sqlite(&url).await.unwrap();
    sqlx::query("INSERT INTO events(event_id,session_id,source,client_type,event_type,occurred_at) VALUES ('a3','a','agent_client','generic','session.exited','2026-01-02')").execute(&reopened).await.unwrap();
    let page = DomainEventReader::new(reopened)
        .read_after(cursor, None, 10)
        .await
        .unwrap();
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].event_id, "a3");
    assert!(page.next_position > cursor);
}

#[tokio::test]
async fn snapshots_include_unsettled_input_without_client_probes() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("snapshot.db").display()
    ))
    .await
    .unwrap();
    run_migrations(&pool).await.unwrap();
    sqlx::raw_sql("INSERT INTO sessions(session_id,client_type,state,current_turn_id) VALUES ('s','generic','busy','t');
        INSERT INTO turns(turn_id,session_id,state) VALUES ('t','s','running');
        INSERT INTO session_runtimes(runtime_id,session_id,role,state,created_at) VALUES ('r','s','tui','running','2026-01-01');
        INSERT INTO inbox_messages(message_id,session_id,state,delivery_policy,input_summary) VALUES ('i','s','unknown','after_idle','task');")
        .execute(&pool).await.unwrap();
    let query = ControlQueryService::new(pool.clone());
    let snapshot = query.session_snapshot("s").await.unwrap().unwrap();
    assert_eq!(snapshot.state, "busy");
    assert_eq!(snapshot.current_turn_id.as_deref(), Some("t"));
    assert_eq!(snapshot.turns[0].state, "running");
    assert_eq!(snapshot.runtimes[0].runtime_id, "r");
    assert!(snapshot.has_unsettled_input);
    assert!(query.verify_active_turn("s", "r").await.is_ok());
    sqlx::query("UPDATE session_runtimes SET state='exited' WHERE runtime_id='r'")
        .execute(&pool)
        .await
        .unwrap();
    let newer = query.session_snapshot("s").await.unwrap().unwrap();
    assert_eq!(newer.event_position, snapshot.event_position);
    assert!(newer.revision > snapshot.revision);
    assert!(matches!(
        query.verify_active_turn("s", "r").await,
        Err(pontia_core::Error::StateConflict(_))
    ));

    assert!(!snapshot.has_agent_binding);
    assert!(query.session_snapshot("missing").await.unwrap().is_none());
}
