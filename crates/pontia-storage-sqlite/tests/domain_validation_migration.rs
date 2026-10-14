use pontia_storage_sqlite::{connect_sqlite, run_migrations};

#[tokio::test]
async fn upgrade_preserves_interactions_and_turn_history() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("upgrade.db").display()
    ))
    .await
    .unwrap();
    for migration in sqlx::migrate!("./migrations")
        .iter()
        .filter(|m| m.version < 34)
    {
        sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
    }
    sqlx::raw_sql(
        r#"INSERT INTO sessions (session_id, client_type, state) VALUES ('session', 'generic', 'idle');
           INSERT INTO turns (turn_id, session_id, state, topology_status) VALUES ('turn', 'session', 'completed', 'root');
           INSERT INTO events (event_id, session_id, turn_id, source, client_type, event_type, occurred_at, turn_topology)
           VALUES ('event', 'session', 'turn', 'agent_adapter', 'generic', 'turn.started', '2026-01-01T00:00:00Z', '{"status":"root"}');"#,
    ).execute(&pool).await.unwrap();
    for state in ["pending", "resolving", "resolved", "expired"] {
        sqlx::query("INSERT INTO interactions VALUES (?, 'session', 'turn', 'choice', 'Choose', 'Description', ?, ?, ?, '2026-01-01T00:00:00Z', ?)")
            .bind(state)
            .bind(r#"[{"key":"allow","label":"Allow","description":null}]"#)
            .bind(state)
            .bind((state == "resolved").then_some("allow"))
            .bind((state == "resolved").then_some("2026-01-01T00:01:00Z"))
            .execute(&pool).await.unwrap();
    }
    let snapshot_sql = "SELECT json_array(interaction_id, session_id, turn_id, type, title, description, options, state, response_option_key, requested_at, resolved_at) FROM interactions ORDER BY interaction_id";
    let before: Vec<String> = sqlx::query_scalar(snapshot_sql)
        .fetch_all(&pool)
        .await
        .unwrap();

    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(include_str!(
        "../migrations/0034_remove_duplicate_domain_validation.sql"
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let after: Vec<String> = sqlx::query_scalar(snapshot_sql)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(after, before);
    let turn: (String, String) =
        sqlx::query_as("SELECT session_id, topology_status FROM turns WHERE turn_id='turn'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(turn, ("session".into(), "root".into()));
    let topology: String =
        sqlx::query_scalar("SELECT turn_topology FROM events WHERE event_id='event'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(topology, r#"{"status":"root"}"#);
    assert!(
        sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn options_validation_is_left_to_application_without_removing_relational_constraints() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("fresh.db").display()
    ))
    .await
    .unwrap();
    run_migrations(&pool).await.unwrap();
    sqlx::query("INSERT INTO sessions (session_id, client_type, state) VALUES ('session', 'generic', 'idle')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO interactions (interaction_id, session_id, type, title, options, state, requested_at) VALUES ('interaction', 'session', 'choice', 'Choose', 'not JSON', 'pending', '2026-01-01T00:00:00Z')").execute(&pool).await.unwrap();
    let options: String =
        sqlx::query_scalar("SELECT options FROM interactions WHERE interaction_id='interaction'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(options, "not JSON");
    assert!(
        sqlx::query(
            "UPDATE interactions SET session_id='missing' WHERE interaction_id='interaction'"
        )
        .execute(&pool)
        .await
        .is_err()
    );
    assert!(
        sqlx::query("UPDATE interactions SET state='resolved' WHERE interaction_id='interaction'")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE interactions SET state='invalid' WHERE interaction_id='interaction'")
            .execute(&pool)
            .await
            .is_err()
    );
    sqlx::query("UPDATE interactions SET state='resolved', response_option_key='allow', resolved_at='2026-01-01T00:01:00Z' WHERE interaction_id='interaction'").execute(&pool).await.unwrap();
    let state: String =
        sqlx::query_scalar("SELECT state FROM interactions WHERE interaction_id='interaction'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, "resolved");
}
