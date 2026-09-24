use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use sqlx::{Column, Row};

#[tokio::test]
async fn removing_submission_snapshots_preserves_messages_and_queue_order() {
    let root = tempfile::tempdir().unwrap();
    let database_url = format!("sqlite://{}", root.path().join("inbox.db").display());
    let pool = connect_sqlite(&database_url).await.unwrap();
    let mut preceding = sqlx::migrate!("./migrations");
    preceding.migrations = preceding
        .iter()
        .filter(|migration| migration.version < 27)
        .cloned()
        .collect::<Vec<_>>()
        .into();
    preceding.run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO sessions(session_id,client_type,state) VALUES ('session','codex','idle')",
    )
    .execute(&pool)
    .await
    .unwrap();
    for (id, state, snapshot, retry_of) in [
        (
            "z-first",
            "failed",
            Some(r#"{"input":"first","metadata":null}"#),
            None,
        ),
        (
            "a-retry",
            "unknown",
            Some("invalid legacy snapshot"),
            Some("z-first"),
        ),
        ("m-pending", "pending", None, None),
    ] {
        sqlx::query("INSERT INTO inbox_messages(message_id,session_id,state,delivery_policy,input_summary,metadata,submission_payload,retry_of_message_id,required_runtime_instance_id,failure_message) VALUES (?,'session',?,'after_idle',?,'{\"codex_turn_id\":\"native\"}',?,?,'runtime','diagnostic')")
            .bind(id).bind(state).bind(format!("body for {id}")).bind(snapshot).bind(retry_of)
            .execute(&pool).await.unwrap();
    }
    let before = sqlx::query("SELECT rowid AS queue_order, * FROM inbox_messages ORDER BY rowid")
        .fetch_all(&pool)
        .await
        .unwrap();
    run_migrations(&pool).await.unwrap();
    pool.close().await;

    let reopened = connect_sqlite(&database_url).await.unwrap();
    run_migrations(&reopened).await.unwrap();
    let after = sqlx::query("SELECT rowid AS queue_order, * FROM inbox_messages ORDER BY rowid")
        .fetch_all(&reopened)
        .await
        .unwrap();
    assert_eq!(after.len(), before.len());
    for (before, after) in before.iter().zip(&after) {
        assert_eq!(
            before.get::<i64, _>("queue_order"),
            after.get::<i64, _>("queue_order")
        );
        assert!(
            !after
                .columns()
                .iter()
                .any(|column| column.name() == "submission_payload")
        );
        for column in before
            .columns()
            .iter()
            .filter(|column| !matches!(column.name(), "queue_order" | "submission_payload"))
        {
            assert_eq!(
                before.get::<Option<String>, _>(column.name()),
                after.get::<Option<String>, _>(column.name()),
                "{}",
                column.name()
            );
        }
    }
    let violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&reopened)
        .await
        .unwrap();
    assert!(violations.is_empty());
}
