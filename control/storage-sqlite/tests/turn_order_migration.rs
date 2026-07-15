use std::borrow::Cow;

use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use sqlx::migrate::Migrator;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

fn migrator_through(version: i64) -> Migrator {
    Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version <= version)
                .cloned()
                .collect(),
        ),
        ignore_missing: false,
        locking: true,
        no_tx: false,
    }
}

async fn historical_pool(name: &str) -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join(name);
    let _kept_dir = dir.keep();
    let pool = connect_sqlite(&format!("sqlite://{}", db_path.display()))
        .await
        .expect("connect");
    migrator_through(35)
        .run(&pool)
        .await
        .expect("migrate through 0035");
    pool
}

#[tokio::test]
async fn migration_backfills_turns_and_event_envelopes_in_previous_order() {
    let pool = historical_pool("turn-order-backfill.db").await;
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state) VALUES ('sess_a', 'pi', 'ready');
           INSERT INTO turns (turn_id, session_id, state, created_at) VALUES
             ('turn_b', 'sess_a', 'completed', '2026-01-01T00:00:00Z'),
             ('turn_a', 'sess_a', 'completed', '2026-01-01T00:00:00Z'),
             ('turn_c', 'sess_a', 'completed', '2026-01-01T00:00:01Z');
           INSERT INTO events
             (event_id, session_id, turn_id, source, client_type, event_type, occurred_at)
           VALUES
             ('evt_a', 'sess_a', 'turn_a', 'agent_client', 'pi', 'turn.completed', '2026-01-01T00:00:00Z'),
             ('evt_b1', 'sess_a', 'turn_b', 'agent_client', 'pi', 'turn.started', '2026-01-01T00:00:00Z'),
             ('evt_b2', 'sess_a', 'turn_b', 'agent_client', 'pi', 'turn.completed', '2026-01-01T00:00:01Z'),
             ('evt_c', 'sess_a', 'turn_c', 'agent_client', 'pi', 'turn.completed', '2026-01-01T00:00:02Z')"#,
    )
    .execute(&pool)
    .await
    .expect("seed historical data");

    MIGRATOR.run(&pool).await.expect("apply 0036");

    let turns: Vec<(String, i64)> = sqlx::query_as(
        "SELECT turn_id, turn_index FROM turns WHERE session_id = 'sess_a' ORDER BY turn_index",
    )
    .fetch_all(&pool)
    .await
    .expect("read turns");
    assert_eq!(
        turns,
        vec![
            ("turn_a".to_string(), 1),
            ("turn_b".to_string(), 2),
            ("turn_c".to_string(), 3),
        ]
    );

    let event_indexes: Vec<(String, i64)> = sqlx::query_as(
        "SELECT event_id, turn_index FROM events WHERE turn_id IS NOT NULL ORDER BY event_id",
    )
    .fetch_all(&pool)
    .await
    .expect("read event indexes");
    assert_eq!(
        event_indexes,
        vec![
            ("evt_a".to_string(), 1),
            ("evt_b1".to_string(), 2),
            ("evt_b2".to_string(), 2),
            ("evt_c".to_string(), 3),
        ]
    );
    let next: i64 =
        sqlx::query_scalar("SELECT next_turn_index FROM sessions WHERE session_id = 'sess_a'")
            .fetch_one(&pool)
            .await
            .expect("next turn index");
    assert_eq!(next, 4);
}

#[tokio::test]
async fn migration_fails_instead_of_guessing_when_turns_and_events_do_not_reconcile() {
    let pool = historical_pool("turn-order-invalid.db").await;
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state) VALUES ('sess_a', 'pi', 'ready');
           INSERT INTO turns (turn_id, session_id, state) VALUES ('turn_without_event', 'sess_a', 'completed')"#,
    )
    .execute(&pool)
    .await
    .expect("seed inconsistent historical data");

    let error = MIGRATOR.run(&pool).await.expect_err("migration must fail");
    assert!(
        error
            .to_string()
            .contains("cannot reconcile historical Turn projections and Turn events"),
        "{error}"
    );
}

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
