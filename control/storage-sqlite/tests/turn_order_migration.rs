use pontia_storage_sqlite::{connect_sqlite, run_migrations};

#[tokio::test]
async fn storage_enforces_unique_and_immutable_session_local_turn_indexes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("turn-order-constraints.db");
    let pool = connect_sqlite(&format!("sqlite://{}", db_path.display()))
        .await
        .expect("connect");
    run_migrations(&pool).await.expect("migrate");
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state) VALUES ('sess_a', 'pi', 'ready');
           INSERT INTO turns (turn_id, session_id, turn_index, state) VALUES
             ('turn_1', 'sess_a', 1, 'completed')"#,
    )
    .execute(&pool)
    .await
    .expect("seed turn");

    let duplicate = sqlx::query(
        "INSERT INTO turns (turn_id, session_id, turn_index, state) VALUES ('turn_2', 'sess_a', 1, 'completed')",
    )
    .execute(&pool)
    .await
    .expect_err("duplicate session-local turn index");
    assert!(duplicate.to_string().contains("UNIQUE constraint failed"));

    let changed = sqlx::query("UPDATE turns SET turn_index = 2 WHERE turn_id = 'turn_1'")
        .execute(&pool)
        .await
        .expect_err("turn index must be immutable");
    assert!(changed.to_string().contains("turn_index are immutable"));

    let mismatched_event = sqlx::query(
        r#"INSERT INTO events
           (event_id, session_id, turn_id, turn_index, source, client_type, event_type, occurred_at)
           VALUES ('evt_mismatch', 'sess_a', 'turn_1', 2, 'agent_client', 'pi', 'turn.completed', '2026-01-01T00:00:00Z')"#,
    )
    .execute(&pool)
    .await
    .expect_err("event and projection indexes must match");
    assert!(
        mismatched_event
            .to_string()
            .contains("must match the Turn projection index")
    );
}
