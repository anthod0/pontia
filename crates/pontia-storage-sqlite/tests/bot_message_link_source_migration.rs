use pontia_storage_sqlite::connect_sqlite;

#[tokio::test]
async fn existing_user_links_are_quotes_and_only_user_inputs_can_have_a_source() {
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    for migration in sqlx::migrate!("./migrations")
        .iter()
        .filter(|m| m.version < 60)
    {
        sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
    }
    sqlx::raw_sql(
        "INSERT INTO bots(bot_id,name,client_type,client_key,model) VALUES ('b','B','generic','default','model');
         INSERT INTO bot_messages(id,bot_id,kind,content) VALUES
           ('first','b','user_input','Hi'), ('reply','b','bot_reply','Hello'), ('follow','b','user_input','Thanks');
         INSERT INTO bot_message_links VALUES ('reply','first'), ('follow','reply');",
    ).execute(&pool).await.unwrap();
    sqlx::raw_sql(include_str!(
        "../migrations/0060_bot_message_link_source.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    let sources: Vec<Option<String>> =
        sqlx::query_scalar("SELECT link_source FROM bot_messages ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(sources, [None, Some("quote".into()), None]);
    for (id, source) in [("reply", "bot"), ("reply", "quote"), ("first", "user")] {
        assert!(
            sqlx::query("UPDATE bot_messages SET link_source = ? WHERE id = ?")
                .bind(source)
                .bind(id)
                .execute(&pool)
                .await
                .unwrap_err()
                .as_database_error()
                .unwrap()
                .is_check_violation()
        );
    }
}
