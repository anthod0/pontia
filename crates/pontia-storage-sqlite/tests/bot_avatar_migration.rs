use pontia_storage_sqlite::connect_sqlite;

#[tokio::test]
async fn existing_bots_receive_default_avatars_without_changing_their_identity() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("avatars.db").display()
    ))
    .await
    .unwrap();
    for migration in sqlx::migrate!("./migrations")
        .iter()
        .filter(|migration| migration.version < 47)
    {
        sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO bots (bot_id, name, client_type, client_key, model, current_session_id, working_on)
        VALUES ('existing', 'Existing Bot', 'generic', 'test', 'model', 'session', 'Task')")
        .execute(&pool).await.unwrap();
    sqlx::raw_sql(include_str!("../migrations/0047_bot_avatars.sql"))
        .execute(&pool)
        .await
        .unwrap();
    let saved: (String, String, String, String, String) = sqlx::query_as(
        "SELECT name, current_session_id, working_on, avatar_shape, avatar_color FROM bots WHERE bot_id = 'existing'"
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(
        saved,
        (
            "Existing Bot".into(),
            "session".into(),
            "Task".into(),
            "classic".into(),
            "blue".into()
        )
    );
}
