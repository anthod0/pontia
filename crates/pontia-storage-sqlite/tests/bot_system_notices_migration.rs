use pontia_storage_sqlite::connect_sqlite;

#[tokio::test]
async fn upgrade_removes_workspace_snapshots_and_only_ephemeral_notifications() {
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    for migration in sqlx::migrate!("./migrations")
        .iter()
        .filter(|m| m.version <= 58)
    {
        sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO bots(bot_id,name,client_type,client_key,model) VALUES ('bot','Bot','generic','default','model')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO bot_known_workspaces VALUES ('bot','ws')")
        .execute(&pool)
        .await
        .unwrap();
    for kind in ["workspace", "todo", "event", "reply"] {
        for delivered in [0, 1] {
            sqlx::query("INSERT INTO bot_notifications(id,bot_id,content,delivered) VALUES (?,'bot','message',?)")
                .bind(format!("{kind}:{delivered}")).bind(delivered).execute(&pool).await.unwrap();
        }
    }
    sqlx::raw_sql(include_str!(
        "../migrations/0059_drop_bot_known_workspaces.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    let known: i64 =
        sqlx::query_scalar("SELECT count(*) FROM sqlite_master WHERE name='bot_known_workspaces'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(known, 0);
    let kept: Vec<String> = sqlx::query_scalar("SELECT id FROM bot_notifications ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(kept, ["event:0", "event:1", "reply:0", "reply:1"]);
}
