use crate::AgentControl;

#[tokio::test]
async fn arming_is_durable_idempotent_and_scoped_to_the_selected_session() {
    let root = tempfile::tempdir().unwrap();
    let pool = pontia_storage_sqlite::connect_sqlite("sqlite::memory:")
        .await
        .unwrap();
    pontia_storage_sqlite::run_migrations(&pool).await.unwrap();
    sqlx::query("INSERT INTO sessions(session_id,client_type,state) VALUES ('selected','generic','busy'),('other','generic','busy')")
        .execute(&pool).await.unwrap();
    let state = AgentControl::builder(pool.clone(), root.path().into()).build();
    state.inbox_commands().stop_scheduling().await;
    for _ in 0..2 {
        state
            .session_commands()
            .exit_when_done("selected")
            .await
            .unwrap();
    }
    assert!(matches!(
        state.session_commands().exit_when_done("missing").await,
        Err(pontia_core::Error::NotFound(_))
    ));
    drop(state);
    let armed: Vec<(String, bool)> =
        sqlx::query_as("SELECT session_id,exit_when_done FROM sessions ORDER BY session_id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(armed, [("other".into(), false), ("selected".into(), true)]);
}
