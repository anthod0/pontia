use pontia_storage_sqlite::{
    connect_sqlite,
    repositories::{
        events::{EventInsertRecord, SqliteEventRepository},
        sessions::{SessionProjectionUpsertRecord, SqliteSessionRepository},
        turns::{SqliteTurnRepository, TurnProjectionUpsertRecord},
    },
    run_migrations,
};
use serde_json::json;

async fn test_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("sqlite_event_ingest_repositories.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let pool = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");
    pool
}

#[tokio::test]
async fn sqlite_event_repository_supports_ingest_event_writes_and_reads() {
    let pool = test_pool().await;
    let repository = SqliteEventRepository::new(pool.clone());

    let mut tx = pool.begin().await.expect("begin tx");
    SqliteEventRepository::insert_event_in_tx(
        &mut tx,
        EventInsertRecord {
            event_id: "evt_1".to_string(),
            session_id: "sess_1".to_string(),
            turn_id: Some("turn_1".to_string()),
            source: "agent_adapter".to_string(),
            client_type: "pi".to_string(),
            event_type: "turn.started".to_string(),
            occurred_at: "2026-06-15T12:00:00Z".to_string(),
            seq: Some(2),
            payload: json!({"input_summary": "hello"}).to_string(),
            turn_index: Some(1),
            timeline_boundary: None,
        },
    )
    .await
    .expect("insert event");
    tx.commit().await.expect("commit tx");

    assert_eq!(
        repository
            .existing_event_state_version("evt_1", "sess_1")
            .await
            .expect("state version"),
        Some(1)
    );
    assert_eq!(
        repository.max_seq("sess_1").await.expect("max seq"),
        Some(2)
    );

    repository
        .record_warnings("evt_1", "sess_1", &["sequence gap".to_string()])
        .await
        .expect("record warnings");

    let rows = repository
        .list_domain_event_rows("sess_1")
        .await
        .expect("list domain event rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].event_id, "evt_1");
    assert_eq!(rows[0].client_type, "pi");
    assert_eq!(rows[0].seq, Some(2));
    assert_eq!(
        rows[0].payload,
        json!({"input_summary": "hello"}).to_string()
    );
}

#[tokio::test]
async fn sqlite_session_and_turn_repositories_support_projection_rows() {
    let pool = test_pool().await;
    let mut tx = pool.begin().await.expect("begin tx");

    SqliteSessionRepository::upsert_projection_in_tx(
        &mut tx,
        SessionProjectionUpsertRecord {
            session_id: "sess_projection".to_string(),
            client_type: "pi".to_string(),
            title: Some("Planning".to_string()),
            handle: Some("planner".to_string()),
            role: Some("planner".to_string()),
            description: Some("Plans work".to_string()),
            execution_profile_id: Some("profile".to_string()),
            execution_profile_version: Some("1".to_string()),
            state: "ready".to_string(),
            current_turn_id: Some("turn_projection".to_string()),
            state_version: 3,
            metadata: json!({"context_usage": {"used": 10}}).to_string(),
        },
    )
    .await
    .expect("upsert session projection");

    SqliteTurnRepository::upsert_projection_in_tx(
        &mut tx,
        TurnProjectionUpsertRecord {
            turn_id: "turn_projection".to_string(),
            session_id: "sess_projection".to_string(),
            turn_index: 1,
            head_cursor: None,
            tail_cursor: None,
            state: "running".to_string(),
            state_version: 3,
            metadata: json!({"input_summary": "do it"}).to_string(),
        },
    )
    .await
    .expect("upsert turn projection");

    tx.commit().await.expect("commit tx");

    let session_rows = SqliteSessionRepository::new(pool.clone())
        .load_projection_rows("sess_projection")
        .await
        .expect("load session projections");
    assert_eq!(session_rows.len(), 1);
    assert_eq!(session_rows[0].state_version, 3);
    assert_eq!(
        session_rows[0].metadata,
        json!({"context_usage": {"used": 10}}).to_string()
    );

    let turn_repository = SqliteTurnRepository::new(pool);
    let turn_rows = turn_repository
        .load_projection_rows("sess_projection")
        .await
        .expect("load turn projections");
    assert_eq!(turn_rows.len(), 1);
    assert_eq!(turn_rows[0].turn_id, "turn_projection");
    assert_eq!(turn_rows[0].state_version, 3);

    let turn = turn_repository
        .get_projection("turn_projection")
        .await
        .expect("get turn projection")
        .expect("turn exists");
    assert_eq!(turn.state, "running");
}
