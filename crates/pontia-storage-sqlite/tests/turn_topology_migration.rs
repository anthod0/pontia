use pontia_storage_sqlite::{connect_sqlite, run_migrations};

#[tokio::test]
async fn turns_without_topology_are_explicitly_unknown() {
    let root = tempfile::tempdir().expect("tempdir");
    let db_path = root.path().join("turn_topology_migration.db");
    let pool = connect_sqlite(&format!("sqlite://{}", db_path.display()))
        .await
        .expect("connect");
    run_migrations(&pool).await.expect("migrate");

    sqlx::query(
        "INSERT INTO sessions (session_id, client_type, state) VALUES ('sess_old', 'pi', 'idle')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO events (event_id, session_id, turn_id, source, client_type, event_type, occurred_at, payload) VALUES ('evt_old', 'sess_old', 'turn_old', 'agent_adapter', 'pi', 'turn.started', '2026-07-16T00:00:00Z', '{}')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO turns (turn_id, session_id, state) VALUES ('turn_old', 'sess_old', 'completed')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let row: (String, Option<String>) = sqlx::query_as(
        "SELECT topology_status, parent_turn_id FROM turns WHERE turn_id = 'turn_old'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row, ("unknown".to_string(), None));
    let event_topology: Option<String> =
        sqlx::query_scalar("SELECT turn_topology FROM events WHERE event_id = 'evt_old'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(event_topology, None);
}
