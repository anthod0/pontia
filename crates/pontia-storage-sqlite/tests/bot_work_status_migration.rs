use pontia_storage_sqlite::connect_sqlite;

#[tokio::test]
async fn existing_bot_identity_and_helpers_survive_removing_reported_work_status() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("status.db").display()
    ))
    .await
    .unwrap();
    for migration in sqlx::migrate!("./migrations")
        .iter()
        .filter(|migration| migration.version < 54)
    {
        sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO bots(bot_id,name,client_type,client_key,model,current_session_id,working_on) VALUES ('bot','Bot','generic','default','model','main','Old work')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO bot_helpers(reference,bot_id,session_id) VALUES ('helper:bot:1','bot','helper')").execute(&pool).await.unwrap();
    sqlx::raw_sql(include_str!("../migrations/0054_bot_work_status.sql"))
        .execute(&pool)
        .await
        .unwrap();
    let saved: (String, String, bool, Option<i64>) = sqlx::query_as("SELECT name,current_session_id,awaiting_reply,awaiting_reply_after FROM bots WHERE bot_id='bot'").fetch_one(&pool).await.unwrap();
    assert_eq!(saved, ("Bot".into(), "main".into(), false, None));
    let helper: String =
        sqlx::query_scalar("SELECT session_id FROM bot_helpers WHERE bot_id='bot'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(helper, "helper");
    assert!(
        sqlx::query("SELECT working_on FROM bots")
            .execute(&pool)
            .await
            .is_err()
    );
}
