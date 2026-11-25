use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use std::borrow::Cow;

#[tokio::test]
async fn upgrade_removes_legacy_tool_tokens_and_preserves_other_environment() {
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    let old = sqlx::migrate::Migrator {
        migrations: Cow::Owned(
            sqlx::migrate!("./migrations")
                .iter()
                .filter(|migration| migration.version < 74)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    old.run(&pool).await.unwrap();
    let environment = json!({"PONTIA_BOT_TOOL_TOKEN":"private-token","KEEP":"value"});
    sqlx::query("INSERT INTO sessions(session_id,client_type,state,metadata,runtime_environment) VALUES ('legacy','codex','exited',?,?)")
        .bind(json!({"model":"test-model","codex_environment":{"PONTIA_BOT_TOOL_TOKEN":"stale-token"}}).to_string())
        .bind(environment.to_string())
        .execute(&pool)
        .await
        .unwrap();
    run_migrations(&pool).await.unwrap();
    let (metadata, runtime): (String, String) = sqlx::query_as(
        "SELECT metadata,runtime_environment FROM sessions WHERE session_id='legacy'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&metadata).unwrap(),
        json!({"model":"test-model"})
    );
    assert_eq!(
        serde_json::from_str::<Value>(&runtime).unwrap(),
        json!({"KEEP":"value"})
    );
}
