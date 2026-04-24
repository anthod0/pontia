use llmparty::storage::sqlite::{connect_sqlite, run_migrations};

#[tokio::test]
async fn connects_to_sqlite_and_runs_migrations() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("control-plane.db");
    let database_url = format!("sqlite://{}", db_path.display());

    let pool = connect_sqlite(&database_url).await.expect("connect sqlite");
    run_migrations(&pool).await.expect("run migrations");

    let migration_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .expect("query migrations");

    assert!(migration_count >= 1);
}
