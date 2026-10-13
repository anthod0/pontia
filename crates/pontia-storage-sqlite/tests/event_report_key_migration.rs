use pontia_storage_sqlite::{connect_sqlite, run_migrations};

#[tokio::test]
async fn report_keys_are_optional_and_unique_within_session_and_event_type() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("report-keys.db").display()
    ))
    .await
    .unwrap();
    run_migrations(&pool).await.unwrap();

    for event_id in ["legacy-1", "legacy-2"] {
        sqlx::query(
            "INSERT INTO events(event_id,session_id,source,client_type,event_type,occurred_at) VALUES (?,'session','agent_client','pi','session.ready','2026-01-01T00:00:00Z')",
        )
        .bind(event_id)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO events(event_id,session_id,turn_id,report_key,source,client_type,event_type,occurred_at) VALUES ('start-1','session','turn-1','entry-1','agent_adapter','pi','turn.started','2026-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let duplicate = sqlx::query(
        "INSERT INTO events(event_id,session_id,turn_id,report_key,source,client_type,event_type,occurred_at) VALUES ('start-2','session','turn-2','entry-1','agent_adapter','pi','turn.started','2026-01-01T00:00:01Z')",
    )
    .execute(&pool)
    .await;
    assert!(duplicate.is_err());

    for (event_id, session_id, event_type) in [
        ("other-session", "other", "turn.started"),
        ("other-type", "session", "turn.output"),
    ] {
        sqlx::query(
            "INSERT INTO events(event_id,session_id,turn_id,report_key,source,client_type,event_type,occurred_at) VALUES (?,?,?,'entry-1','agent_adapter','pi',?,'2026-01-01T00:00:02Z')",
        )
        .bind(event_id)
        .bind(session_id)
        .bind(format!("turn-{event_id}"))
        .bind(event_type)
        .execute(&pool)
        .await
        .unwrap();
    }
}
