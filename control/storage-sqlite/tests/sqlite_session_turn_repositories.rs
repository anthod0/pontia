use pontia_storage_sqlite::{
    connect_sqlite,
    repositories::{sessions::SqliteSessionRepository, turns::SqliteTurnRepository},
    run_migrations,
};
use serde_json::json;

async fn test_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("sqlite_session_turn_repositories.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let pool = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");
    pool
}

#[tokio::test]
async fn sqlite_session_repository_lists_sessions_with_workspace_coalescing_and_existing_order() {
    let pool = test_pool().await;
    sqlx::query(
        r#"INSERT INTO workspaces (workspace_id, canonical_path, display_path, name)
           VALUES ('ws_1', '/canonical', '/canonical', 'canonical')"#,
    )
    .execute(&pool)
    .await
    .expect("insert workspace");
    sqlx::query(
        r#"INSERT INTO sessions
           (session_id, client_type, title, handle, role, description, execution_profile_id,
            execution_profile_version, state, current_turn_id, workspace_ref, workspace_id,
            metadata, created_at, updated_at)
           VALUES
           ('sess_b', 'pi', 'B', 'b', 'worker', 'desc b', 'profile', '1', 'ready', NULL,
            '/legacy-b', NULL, ?, '2026-06-15T12:00:01Z', '2026-06-15T12:00:01Z'),
           ('sess_a', 'pi', 'A', 'a', 'planner', 'desc a', 'profile', '1', 'ready', NULL,
            '/legacy-a', 'ws_1', ?, '2026-06-15T12:00:00Z', '2026-06-15T12:00:00Z')"#,
    )
    .bind(json!({"model": "grok"}).to_string())
    .bind(json!({"context_usage": {"used": 1}}).to_string())
    .execute(&pool)
    .await
    .expect("insert sessions");

    let repository = SqliteSessionRepository::new(pool);
    let rows = repository.list_sessions().await.expect("list sessions");

    let ids: Vec<_> = rows.iter().map(|row| row.session_id.as_str()).collect();
    assert_eq!(ids, vec!["sess_a", "sess_b"]);
    assert_eq!(rows[0].workspace_ref.as_deref(), Some("/canonical"));
    assert_eq!(rows[1].workspace_ref.as_deref(), Some("/legacy-b"));
}

#[tokio::test]
async fn sqlite_session_repository_finds_active_session_handle_conflicts_only() {
    let pool = test_pool().await;
    sqlx::query(
        r#"INSERT INTO workspaces (workspace_id, canonical_path, display_path, name)
           VALUES ('ws_1', '/one', '/one', 'one'), ('ws_2', '/two', '/two', 'two')"#,
    )
    .execute(&pool)
    .await
    .expect("insert workspaces");
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, handle, workspace_id, state, metadata)
           VALUES
           ('sess_active', 'pi', 'planner', 'ws_1', 'ready', '{}'),
           ('sess_exited', 'pi', 'planner', 'ws_2', 'exited', '{}')"#,
    )
    .execute(&pool)
    .await
    .expect("insert sessions");

    let repository = SqliteSessionRepository::new(pool);

    assert_eq!(
        repository
            .active_session_id_for_handle("ws_1", "planner")
            .await
            .expect("find active handle"),
        Some("sess_active".to_string())
    );
    assert_eq!(
        repository
            .active_session_id_for_handle("ws_2", "planner")
            .await
            .expect("ignore terminal handle"),
        None
    );
}

#[tokio::test]
async fn sqlite_session_repository_updates_workspace_binding() {
    let pool = test_pool().await;
    sqlx::query(
        r#"INSERT INTO workspaces (workspace_id, canonical_path, display_path, name)
           VALUES ('ws_old', '/old-canonical', '/old-canonical', 'old'),
                  ('ws_new', '/new', '/new', 'new')"#,
    )
    .execute(&pool)
    .await
    .expect("insert workspaces");
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, workspace_ref, workspace_id, state, metadata)
           VALUES ('sess_workspace', 'pi', '/old', 'ws_old', 'ready', '{}')"#,
    )
    .execute(&pool)
    .await
    .expect("insert session");

    let repository = SqliteSessionRepository::new(pool);
    repository
        .update_session_workspace("sess_workspace", Some("/new"), Some("ws_new"))
        .await
        .expect("update workspace");

    let row = repository
        .get_session("sess_workspace")
        .await
        .expect("get session")
        .expect("session exists");
    assert_eq!(row.workspace_ref.as_deref(), Some("/new"));
    assert_eq!(row.workspace_id.as_deref(), Some("ws_new"));
}

#[tokio::test]
async fn sqlite_turn_repository_lists_turns_and_event_rows_with_existing_order() {
    let pool = test_pool().await;
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state, metadata)
           VALUES ('sess_turns', 'pi', 'ready', '{}')"#,
    )
    .execute(&pool)
    .await
    .expect("insert session");
    sqlx::query(
        r#"INSERT INTO turns
           (turn_id, session_id, state, input_summary, output_summary, failure_message,
            metadata, created_at, updated_at)
           VALUES
           ('turn_b', 'sess_turns', 'queued', 'input b', NULL, NULL, ?,
            '2026-06-15T12:00:01Z', '2026-06-15T12:00:01Z'),
           ('turn_a', 'sess_turns', 'completed', 'input a', 'output a', NULL, ?,
            '2026-06-15T12:00:00Z', '2026-06-15T12:00:00Z')"#,
    )
    .bind(json!({"artifact_ids": ["artifact_b"]}).to_string())
    .bind(json!({"artifact_ids": ["artifact_a"]}).to_string())
    .execute(&pool)
    .await
    .expect("insert turns");
    sqlx::query(
        r#"INSERT INTO events
           (event_id, session_id, turn_id, source, client_type, event_type, occurred_at, payload)
           VALUES
           ('evt_b', 'sess_turns', 'turn_a', 'client', 'pi', 'turn.output', '2026-06-15T12:00:00Z', ?),
           ('evt_a', 'sess_turns', 'turn_a', 'client', 'pi', 'turn.started', '2026-06-15T12:00:00Z', ?)"#,
    )
    .bind(json!({"output_summary": "from event"}).to_string())
    .bind(json!({"input_summary": "from event"}).to_string())
    .execute(&pool)
    .await
    .expect("insert events");

    let repository = SqliteTurnRepository::new(pool);
    let rows = repository
        .list_turns("sess_turns")
        .await
        .expect("list turns");
    let ids: Vec<_> = rows.iter().map(|row| row.turn_id.as_str()).collect();
    assert_eq!(ids, vec!["turn_a", "turn_b"]);
    assert_eq!(
        rows[0].metadata,
        json!({"artifact_ids": ["artifact_a"]}).to_string()
    );

    let event_rows = repository
        .list_turn_event_enrichment_rows("sess_turns", "turn_a")
        .await
        .expect("list turn events");
    let event_ids: Vec<_> = event_rows.iter().map(|row| row.event_id.as_str()).collect();
    assert_eq!(event_ids, vec!["evt_b", "evt_a"]);
}
