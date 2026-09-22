use pontia_storage_sqlite::connect_sqlite;

#[tokio::test]
async fn migration_backfills_current_turn_from_latest_started_turn_index() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("current-turn-leaf-migration.db");
    let pool = connect_sqlite(&format!("sqlite://{}", db_path.display()))
        .await
        .expect("connect");

    sqlx::raw_sql(include_str!("../migrations/0001_init.sql"))
        .execute(&pool)
        .await
        .expect("initialize baseline schema");
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state, current_turn_id, metadata)
           VALUES
           ('sess_started', 'pi', 'idle', NULL, '{}'),
           ('sess_never_started', 'pi', 'idle', 'turn_queued_only', '{}')"#,
    )
    .execute(&pool)
    .await
    .expect("insert sessions");
    sqlx::query(
        r#"INSERT INTO turns (turn_id, session_id, turn_index, state, metadata)
           VALUES
           ('turn_1', 'sess_started', 1, 'completed', '{}'),
           ('turn_2', 'sess_started', 2, 'completed', '{}'),
           ('turn_3', 'sess_started', 3, 'queued', '{}'),
           ('turn_queued_only', 'sess_never_started', 1, 'queued', '{}')"#,
    )
    .execute(&pool)
    .await
    .expect("insert turns");
    sqlx::query(
        r#"INSERT INTO events
           (event_id, session_id, turn_id, source, client_type, event_type, occurred_at, payload, turn_index)
           VALUES
           ('evt_turn_1_started', 'sess_started', 'turn_1', 'agent_client', 'pi', 'turn.started', '2030-01-01T00:00:00Z', '{}', 1),
           ('evt_turn_2_started', 'sess_started', 'turn_2', 'agent_client', 'pi', 'turn.started', '2020-01-01T00:00:00Z', '{}', 2),
           ('evt_turn_3_queued', 'sess_started', 'turn_3', 'external_api', 'pi', 'turn.queued', '2040-01-01T00:00:00Z', '{}', 3)"#,
    )
    .execute(&pool)
    .await
    .expect("insert events");

    sqlx::raw_sql(include_str!(
        "../migrations/0003_backfill_current_turn_branch_leaf.sql"
    ))
    .execute(&pool)
    .await
    .expect("backfill current branch leaf");

    let started_leaf: Option<String> = sqlx::query_scalar(
        "SELECT current_turn_id FROM sessions WHERE session_id = 'sess_started'",
    )
    .fetch_one(&pool)
    .await
    .expect("load started leaf");
    let never_started_leaf: Option<String> = sqlx::query_scalar(
        "SELECT current_turn_id FROM sessions WHERE session_id = 'sess_never_started'",
    )
    .fetch_one(&pool)
    .await
    .expect("load never-started leaf");

    assert_eq!(started_leaf.as_deref(), Some("turn_2"));
    assert_eq!(never_started_leaf, None);
}
