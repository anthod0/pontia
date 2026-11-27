use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use sqlx::Row;

#[tokio::test]
async fn migration_keeps_only_pending_messages_in_the_five_column_queue() {
    let test_root = tempfile::tempdir().unwrap();
    let database_url = format!(
        "sqlite://{}",
        test_root.path().join("inbox-queue.db").display()
    );
    let pool = connect_sqlite(&database_url).await.unwrap();
    let mut preceding = sqlx::migrate!("./migrations");
    preceding.migrations = preceding
        .iter()
        .filter(|migration| migration.version < 78)
        .cloned()
        .collect::<Vec<_>>()
        .into();
    preceding.run(&pool).await.unwrap();
    sqlx::raw_sql(
        r#"
        INSERT INTO sessions(session_id,client_type,state) VALUES ('session','pi','idle');
        INSERT INTO inbox_messages(message_id,session_id,state,delivery_policy,input_summary,metadata,created_at)
        VALUES
            ('pending','session','pending','after_idle','queued','{"source":"user"}','2026-01-01T00:00:00.000Z'),
            ('dispatching','session','dispatching','after_idle','in flight','{}','2026-01-01T00:00:01.000Z'),
            ('delivered','session','dispatched','after_idle','sent','{}','2026-01-01T00:00:02.000Z'),
            ('failed','session','failed','after_idle','failed','{}','2026-01-01T00:00:03.000Z'),
            ('cancelled','session','cancelled','after_idle','cancelled','{}','2026-01-01T00:00:04.000Z');
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    run_migrations(&pool).await.unwrap();

    let columns = sqlx::query("PRAGMA table_info(inbox_messages)")
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect::<Vec<_>>();
    assert_eq!(
        columns,
        [
            "message_id",
            "session_id",
            "input_summary",
            "metadata",
            "created_at"
        ]
    );
    let rows = sqlx::query("SELECT * FROM inbox_messages")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String, _>("message_id"), "pending");
    assert_eq!(rows[0].get::<String, _>("input_summary"), "queued");
    assert_eq!(rows[0].get::<String, _>("metadata"), r#"{"source":"user"}"#);
    assert_eq!(
        rows[0].get::<String, _>("created_at"),
        "2026-01-01T00:00:00.000Z"
    );
    assert_eq!(rows[0].columns().len(), 5);
}
