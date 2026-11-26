use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use std::borrow::Cow;

#[tokio::test]
async fn upgrade_preserves_choice_responses_and_accepts_extended_input_types() {
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    let old = sqlx::migrate::Migrator {
        migrations: Cow::Owned(
            sqlx::migrate!("./migrations")
                .iter()
                .filter(|migration| migration.version < 76)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    old.run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO sessions(session_id,client_type,state) VALUES('session','generic','idle')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO interactions(interaction_id,session_id,type,title,options,state,response_option_key,requested_at,resolved_at) VALUES('choice','session','choice','Choose','[]','resolved','allow','2026-11-27T00:00:00Z','2026-11-27T00:01:00Z')")
        .execute(&pool)
        .await
        .unwrap();

    run_migrations(&pool).await.unwrap();

    let (interaction_type, allows_custom_text, response): (String, bool, String) = sqlx::query_as(
        "SELECT type,allows_custom_text,response FROM interactions WHERE interaction_id='choice'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(interaction_type, "choice");
    assert!(!allows_custom_text);
    assert_eq!(
        serde_json::from_str::<Value>(&response).unwrap(),
        json!({"option_key":"allow"})
    );

    for (id, interaction_type) in [("multi", "multi_select"), ("text", "text")] {
        sqlx::query("INSERT INTO interactions(interaction_id,session_id,type,title,options,allows_custom_text,state,requested_at) VALUES(?,'session',?,'Question','[]',1,'pending','2026-11-27T00:02:00Z')")
            .bind(id)
            .bind(interaction_type)
            .execute(&pool)
            .await
            .unwrap();
    }
}
