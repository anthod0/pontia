use pontia_storage_sqlite::{connect_sqlite, run_migrations};

async fn pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("turn_topology_migration.db");
    let _kept_dir = dir.keep();
    connect_sqlite(&format!("sqlite://{}", db_path.display()))
        .await
        .expect("connect")
}

#[tokio::test]
async fn turns_without_topology_are_explicitly_unknown() {
    let pool = pool().await;
    run_migrations(&pool).await.expect("migrate");

    sqlx::query(
        "INSERT INTO sessions (session_id, client_type, state) VALUES ('sess_old', 'pi', 'idle')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO events (event_id, session_id, turn_id, source, client_type, event_type, occurred_at, payload, turn_index) VALUES ('evt_old', 'sess_old', 'turn_old', 'agent_adapter', 'pi', 'turn.started', '2026-07-16T00:00:00Z', '{}', 1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO turns (turn_id, session_id, turn_index, state) VALUES ('turn_old', 'sess_old', 1, 'completed')",
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

#[tokio::test]
async fn sqlite_enforces_turn_topology_invariants_and_write_once_resolution() {
    let pool = pool().await;
    run_migrations(&pool).await.expect("migrate");
    for session_id in ["sess_a", "sess_b"] {
        sqlx::query(
            "INSERT INTO sessions (session_id, client_type, state) VALUES (?, 'generic', 'idle')",
        )
        .bind(session_id)
        .execute(&pool)
        .await
        .unwrap();
    }

    sqlx::query("INSERT INTO turns (turn_id, session_id, turn_index, state, topology_status) VALUES ('root_a', 'sess_a', 1, 'completed', 'root')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO turns (turn_id, session_id, turn_index, state, topology_status) VALUES ('root_b', 'sess_b', 1, 'completed', 'root')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO turns (turn_id, session_id, turn_index, state, topology_status) VALUES ('future_a', 'sess_a', 3, 'completed', 'unknown')")
        .execute(&pool)
        .await
        .unwrap();

    for statement in [
        "INSERT INTO turns (turn_id, session_id, turn_index, state, topology_status, parent_turn_id) VALUES ('bad_missing', 'sess_a', 2, 'running', 'linked', NULL)",
        "INSERT INTO turns (turn_id, session_id, turn_index, state, topology_status, parent_turn_id) VALUES ('bad_self', 'sess_a', 2, 'running', 'linked', 'bad_self')",
        "INSERT INTO turns (turn_id, session_id, turn_index, state, topology_status, parent_turn_id) VALUES ('bad_forward', 'sess_a', 2, 'running', 'linked', 'future_a')",
        "INSERT INTO turns (turn_id, session_id, turn_index, state, topology_status, parent_turn_id) VALUES ('bad_cross', 'sess_a', 2, 'running', 'linked', 'root_b')",
        "INSERT INTO turns (turn_id, session_id, turn_index, state, topology_status, parent_turn_id) VALUES ('bad_root', 'sess_a', 2, 'running', 'root', 'root_a')",
    ] {
        assert!(
            sqlx::query(statement).execute(&pool).await.is_err(),
            "{statement}"
        );
    }

    sqlx::query("INSERT INTO turns (turn_id, session_id, turn_index, state, topology_status, parent_turn_id) VALUES ('child_a', 'sess_a', 2, 'running', 'linked', 'root_a')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE turns SET topology_status = 'linked', parent_turn_id = 'root_a' WHERE turn_id = 'child_a'")
        .execute(&pool)
        .await
        .expect("identical topology write is idempotent");
    assert!(
        sqlx::query("UPDATE turns SET topology_status = 'root', parent_turn_id = NULL WHERE turn_id = 'child_a'")
            .execute(&pool)
            .await
            .is_err()
    );
}
