use pontia_application::AppState;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};

#[tokio::test]
async fn app_state_is_constructed_through_builder() {
    let db = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&db).await.unwrap();

    let state = AppState::builder(db.clone()).build();

    let fetched: i64 = sqlx::query_scalar("SELECT 1")
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert_eq!(fetched, 1);
}
