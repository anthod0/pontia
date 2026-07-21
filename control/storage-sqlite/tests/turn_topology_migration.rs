use pontia_storage_sqlite::{connect_sqlite, run_migrations};

const ROOT_A: &str = "turn_01900000-0000-7000-8000-000000000001";
const CHILD_A: &str = "turn_01900000-0000-7000-8000-000000000002";
const FUTURE_A: &str = "turn_01900000-0000-7000-8000-000000000003";
const ROOT_B: &str = "turn_01900000-0000-7000-8000-000000000004";

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
        "INSERT INTO events (event_id, session_id, turn_id, source, client_type, event_type, occurred_at, payload) VALUES ('evt_old', 'sess_old', ?, 'agent_adapter', 'pi', 'turn.started', '2026-07-16T00:00:00Z', '{}')",
    )
    .bind(ROOT_A)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO turns (turn_id, session_id, state) VALUES (?, 'sess_old', 'completed')",
    )
    .bind(ROOT_A)
    .execute(&pool)
    .await
    .unwrap();

    let row: (String, Option<String>) =
        sqlx::query_as("SELECT topology_status, parent_turn_id FROM turns WHERE turn_id = ?")
            .bind(ROOT_A)
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

    for (turn_id, session_id, topology) in [
        (ROOT_A, "sess_a", "root"),
        (ROOT_B, "sess_b", "root"),
        (FUTURE_A, "sess_a", "unknown"),
    ] {
        sqlx::query(
            "INSERT INTO turns (turn_id, session_id, state, topology_status) VALUES (?, ?, 'completed', ?)",
        )
        .bind(turn_id)
        .bind(session_id)
        .bind(topology)
        .execute(&pool)
        .await
        .unwrap();
    }

    for (label, topology, parent) in [
        ("missing", "linked", None),
        ("self", "linked", Some(CHILD_A)),
        ("forward", "linked", Some(FUTURE_A)),
        ("cross-session", "linked", Some(ROOT_B)),
        ("root-with-parent", "root", Some(ROOT_A)),
    ] {
        let result = sqlx::query(
            "INSERT INTO turns (turn_id, session_id, state, topology_status, parent_turn_id) VALUES (?, 'sess_a', 'running', ?, ?)",
        )
        .bind(CHILD_A)
        .bind(topology)
        .bind(parent)
        .execute(&pool)
        .await;
        assert!(result.is_err(), "{label}");
    }

    for (label, parent_turn_id) in [("forward", FUTURE_A), ("cross-session", ROOT_B)] {
        let topology = serde_json::json!({
            "status": "linked",
            "parent_turn_id": parent_turn_id,
        })
        .to_string();
        let result = sqlx::query(
            "INSERT INTO events (event_id, session_id, turn_id, source, client_type, event_type, occurred_at, turn_topology) VALUES (?, 'sess_a', ?, 'agent_adapter', 'pi', 'turn.started', '2026-07-16T00:00:00Z', ?)",
        )
        .bind(format!("evt_invalid_{label}"))
        .bind(CHILD_A)
        .bind(topology)
        .execute(&pool)
        .await;
        assert!(result.is_err(), "{label} event parent must be rejected");
    }

    let linked_topology = serde_json::json!({
        "status": "linked",
        "parent_turn_id": ROOT_A,
    })
    .to_string();
    sqlx::query(
        "INSERT INTO events (event_id, session_id, turn_id, source, client_type, event_type, occurred_at, turn_topology) VALUES ('evt_child', 'sess_a', ?, 'agent_adapter', 'pi', 'turn.started', '2026-07-16T00:00:00Z', ?)",
    )
    .bind(CHILD_A)
    .bind(linked_topology)
    .execute(&pool)
    .await
    .expect("linked event parent precedes child by UUIDv7 Turn ID");

    sqlx::query(
        "INSERT INTO turns (turn_id, session_id, state, topology_status, parent_turn_id) VALUES (?, 'sess_a', 'running', 'linked', ?)",
    )
    .bind(CHILD_A)
    .bind(ROOT_A)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE turns SET topology_status = 'linked', parent_turn_id = ? WHERE turn_id = ?",
    )
    .bind(ROOT_A)
    .bind(CHILD_A)
    .execute(&pool)
    .await
    .expect("identical topology write is idempotent");
    assert!(
        sqlx::query(
            "UPDATE turns SET topology_status = 'root', parent_turn_id = NULL WHERE turn_id = ?",
        )
        .bind(CHILD_A)
        .execute(&pool)
        .await
        .is_err()
    );
}
