use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use sqlx::Row;

#[tokio::test]
async fn migration_removes_turn_ordinals_and_preserves_uuid_v7_identity_invariants() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("turn-order-constraints.db");
    let pool = connect_sqlite(&format!("sqlite://{}", db_path.display()))
        .await
        .expect("connect");
    run_migrations(&pool).await.expect("migrate");

    for table in ["events", "turns", "sessions"] {
        let columns = sqlx::query(&format!("PRAGMA table_info({table})"))
            .fetch_all(&pool)
            .await
            .expect("inspect columns")
            .into_iter()
            .map(|row| row.get::<String, _>("name"))
            .collect::<Vec<_>>();
        assert!(!columns.iter().any(|column| column == "turn_index"));
        assert!(!columns.iter().any(|column| column == "next_turn_index"));
    }

    sqlx::raw_sql(
        r#"INSERT INTO sessions (session_id, client_type, state) VALUES
             ('sess_a', 'pi', 'ready'),
             ('sess_b', 'pi', 'ready');
           INSERT INTO turns (turn_id, session_id, state, topology_status) VALUES
             ('turn_01900000-0000-7000-8000-000000000001', 'sess_a', 'completed', 'root'),
             ('turn_01900000-0000-7000-8000-000000000003', 'sess_a', 'completed', 'unknown'),
             ('turn_01900000-0000-7000-8000-000000000004', 'sess_b', 'completed', 'root');"#,
    )
    .execute(&pool)
    .await
    .expect("seed turns");

    let changed = sqlx::query(
        "UPDATE turns SET session_id = 'sess_b' WHERE turn_id = 'turn_01900000-0000-7000-8000-000000000001'",
    )
    .execute(&pool)
    .await
    .expect_err("Turn session must be immutable");
    assert!(changed.to_string().contains("session_id is immutable"));

    let missing_turn_id = sqlx::query(
        "INSERT INTO events (event_id, session_id, source, client_type, event_type, occurred_at) VALUES ('evt_missing_turn', 'sess_a', 'agent_client', 'pi', 'turn.completed', '2026-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect_err("Turn events require a Turn ID");
    assert!(missing_turn_id.to_string().contains("turn_id is required"));

    for (label, parent_id) in [
        ("forward", "turn_01900000-0000-7000-8000-000000000003"),
        ("cross-session", "turn_01900000-0000-7000-8000-000000000004"),
    ] {
        let result = sqlx::query(
            "INSERT INTO turns (turn_id, session_id, state, topology_status, parent_turn_id) VALUES ('turn_01900000-0000-7000-8000-000000000002', 'sess_a', 'running', 'linked', ?)",
        )
        .bind(parent_id)
        .execute(&pool)
        .await;
        assert!(result.is_err(), "{label} parent must be rejected");
    }

    sqlx::query(
        "INSERT INTO turns (turn_id, session_id, state, topology_status, parent_turn_id) VALUES ('turn_01900000-0000-7000-8000-000000000002', 'sess_a', 'running', 'linked', 'turn_01900000-0000-7000-8000-000000000001')",
    )
    .execute(&pool)
    .await
    .expect("earlier same-session parent");
}

#[tokio::test]
async fn migration_upgrades_legacy_rows_without_losing_events_or_turns() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("turn-order-upgrade.db");
    let pool = connect_sqlite(&format!("sqlite://{}", db_path.display()))
        .await
        .expect("connect");

    for migration in [
        include_str!("../migrations/0001_init.sql"),
        include_str!("../migrations/0002_drop_idempotency_keys.sql"),
        include_str!("../migrations/0003_backfill_current_turn_branch_leaf.sql"),
        include_str!("../migrations/0004_drop_turn_projection_event_index_trigger.sql"),
    ] {
        sqlx::raw_sql(migration)
            .execute(&pool)
            .await
            .expect("apply legacy migration");
    }
    sqlx::raw_sql(
        r#"INSERT INTO sessions (session_id, client_type, state, next_turn_index)
           VALUES ('sess_a', 'pi', 'idle', 2);
           INSERT INTO events
             (event_id, session_id, turn_id, source, client_type, event_type, occurred_at, payload, turn_index)
           VALUES
             ('evt_1', 'sess_a', 'turn_01900000-0000-7000-8000-000000000001', 'agent_client', 'pi', 'turn.started', '2026-01-01T00:00:00Z', '{}', 1);
           INSERT INTO turns (turn_id, session_id, turn_index, state)
           VALUES ('turn_01900000-0000-7000-8000-000000000001', 'sess_a', 1, 'completed');"#,
    )
    .execute(&pool)
    .await
    .expect("seed legacy rows");

    sqlx::raw_sql(include_str!("../migrations/0005_remove_turn_indexes.sql"))
        .execute(&pool)
        .await
        .expect("remove legacy ordinals");

    let event_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_id = 'evt_1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let turn_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM turns WHERE turn_id = 'turn_01900000-0000-7000-8000-000000000001'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((event_count, turn_count), (1, 1));
}
