use pontia_storage_sqlite::connect_sqlite;
use serde_json::{Value, json};

#[tokio::test]
async fn each_existing_shared_file_becomes_its_own_share() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("shares.db").display()
    ))
    .await
    .unwrap();
    for migration in sqlx::migrate!("./migrations")
        .iter()
        .filter(|migration| migration.version < 55)
    {
        sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
    }
    let uuid = "019a0000-0000-7000-8000-000000000001";
    sqlx::query(
        "INSERT INTO sessions (session_id, client_type, state) VALUES ('s', 'generic', 'idle')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO session_files (file_id, session_id, path, name, size, media_type, created_at) VALUES (?, 's', '/a.txt', 'a.txt', 1, 'application/octet-stream', '2026-01-01T00:00:00Z')")
        .bind(format!("file_{uuid}")).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO events (event_id, session_id, source, client_type, event_type, occurred_at, payload) VALUES ('e', 's', 'external_api', 'generic', 'session.file_shared', '2026-01-01T00:00:00Z', ?)")
        .bind(json!({"file_id": format!("file_{uuid}")}).to_string()).execute(&pool).await.unwrap();

    sqlx::raw_sql(include_str!("../migrations/0055_session_file_shares.sql"))
        .execute(&pool)
        .await
        .unwrap();

    let share: String = sqlx::query_scalar("SELECT share_id FROM session_files")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(share, format!("share_{uuid}"));
    let (event_type, payload): (String, String) =
        sqlx::query_as("SELECT event_type, payload FROM events WHERE event_id = 'e'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(event_type, "session.files_shared");
    assert_eq!(
        serde_json::from_str::<Value>(&payload).unwrap(),
        json!({"share_id": format!("share_{uuid}"), "file_ids": [format!("file_{uuid}")]})
    );
}
