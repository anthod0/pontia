use pontia_storage_sqlite::{
    connect_sqlite,
    repositories::{sessions::SqliteSessionRepository, turns::SqliteTurnRepository},
    run_migrations,
};
use serde_json::json;

async fn test_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("sqlite_session_turn_repositories.db");
    let database_url = format!("sqlite://{}", db_path.display());
    let pool = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");
    (pool, dir)
}

#[tokio::test]
async fn sqlite_session_repository_finds_active_session_handle_conflicts_only() {
    let (pool, _pontia_home) = test_pool().await;
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
           ('sess_active', 'pi', 'reviewer', 'ws_1', 'ready', '{}'),
           ('sess_exited', 'pi', 'reviewer', 'ws_2', 'exited', '{}')"#,
    )
    .execute(&pool)
    .await
    .expect("insert sessions");

    let repository = SqliteSessionRepository::new(pool);

    assert_eq!(
        repository
            .active_session_id_for_handle("ws_1", "reviewer")
            .await
            .expect("find active handle"),
        Some("sess_active".to_string())
    );
    assert_eq!(
        repository
            .active_session_id_for_handle("ws_2", "reviewer")
            .await
            .expect("ignore terminal handle"),
        None
    );
}

#[tokio::test]
async fn sqlite_session_repository_updates_workspace_binding() {
    let (pool, _pontia_home) = test_pool().await;
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
async fn sqlite_session_management_preserves_the_last_activity_time() {
    let (pool, _pontia_home) = test_pool().await;
    let last_activity = "2026-06-15T12:00:00Z";
    sqlx::query(
        r#"INSERT INTO sessions
           (session_id, client_type, state, metadata, created_at, updated_at)
           VALUES ('sess_managed', 'pi', 'ready', '{}', ?, ?)"#,
    )
    .bind(last_activity)
    .bind(last_activity)
    .execute(&pool)
    .await
    .expect("insert session");

    let repository = SqliteSessionRepository::new(pool);

    repository
        .pin_session("sess_managed")
        .await
        .expect("pin session");
    let pinned = repository
        .get_session("sess_managed")
        .await
        .expect("get pinned session")
        .expect("session exists");
    assert!(pinned.pinned_at.is_some());
    assert_eq!(pinned.updated_at, last_activity);

    repository
        .unpin_session("sess_managed")
        .await
        .expect("unpin session");
    let unpinned = repository
        .get_session("sess_managed")
        .await
        .expect("get unpinned session")
        .expect("session exists");
    assert!(unpinned.pinned_at.is_none());
    assert_eq!(unpinned.updated_at, last_activity);

    repository
        .pin_session("sess_managed")
        .await
        .expect("repin session");
    repository
        .archive_session("sess_managed")
        .await
        .expect("archive session");
    let archived = repository
        .get_session("sess_managed")
        .await
        .expect("get archived session")
        .expect("session exists");
    assert!(archived.pinned_at.is_none());
    assert!(archived.archived_at.is_some());
    assert_eq!(archived.updated_at, last_activity);

    repository
        .unarchive_session("sess_managed")
        .await
        .expect("unarchive session");
    let unarchived = repository
        .get_session("sess_managed")
        .await
        .expect("get unarchived session")
        .expect("session exists");
    assert!(unarchived.archived_at.is_none());
    assert_eq!(unarchived.updated_at, last_activity);
}

#[tokio::test]
async fn sqlite_turn_repository_resolves_the_unique_active_turn() {
    let (pool, _pontia_home) = test_pool().await;
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state, metadata)
           VALUES ('sess_active_turn', 'pi', 'idle', '{}')"#,
    )
    .execute(&pool)
    .await
    .expect("insert session");
    sqlx::query(
        r#"INSERT INTO turns (turn_id, session_id, state, metadata)
           VALUES
           ('turn_completed', 'sess_active_turn', 'completed', '{}'),
           ('turn_running', 'sess_active_turn', 'running', '{}')"#,
    )
    .execute(&pool)
    .await
    .expect("insert turns");

    let repository = SqliteTurnRepository::new(pool);
    let active = repository
        .active_turn("sess_active_turn")
        .await
        .expect("resolve active turn")
        .expect("active turn exists");

    assert_eq!(active.turn_id, "turn_running");
    assert_eq!(active.state, "running");
}

#[tokio::test]
async fn sqlite_turn_repository_rejects_multiple_active_turns() {
    let (pool, _pontia_home) = test_pool().await;
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state, metadata)
           VALUES ('sess_invalid_active_turns', 'pi', 'busy', '{}')"#,
    )
    .execute(&pool)
    .await
    .expect("insert session");
    sqlx::query(
        r#"INSERT INTO turns (turn_id, session_id, state, metadata)
           VALUES
           ('turn_queued', 'sess_invalid_active_turns', 'queued', '{}'),
           ('turn_running', 'sess_invalid_active_turns', 'running', '{}')"#,
    )
    .execute(&pool)
    .await
    .expect("insert turns");

    let error = SqliteTurnRepository::new(pool)
        .active_turn("sess_invalid_active_turns")
        .await
        .expect_err("multiple active turns must violate the invariant");

    assert!(error.to_string().contains("multiple active Turns"));
}

#[tokio::test]
async fn sqlite_turn_repository_lists_turns_and_event_rows_with_existing_order() {
    let (pool, _pontia_home) = test_pool().await;
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state, metadata)
           VALUES ('sess_turns', 'pi', 'ready', '{}')"#,
    )
    .execute(&pool)
    .await
    .expect("insert session");
    sqlx::query(
        r#"INSERT INTO turns
           (turn_id, session_id, state, input_summary, output_summary,
            metadata, created_at, updated_at)
           VALUES
           ('turn_b', 'sess_turns', 'queued', 'input b', NULL, ?,
            '2026-06-15T12:00:01Z', '2026-06-15T12:00:01Z'),
           ('turn_a', 'sess_turns', 'completed', 'input a', 'output a', ?,
            '2026-06-15T12:00:00Z', '2026-06-15T12:00:00Z')"#,
    )
    .bind(json!({"note": "b"}).to_string())
    .bind(json!({"note": "a"}).to_string())
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
    assert_eq!(rows[0].metadata, json!({"note": "a"}).to_string());

    let event_rows = repository
        .list_turn_event_enrichment_rows("sess_turns", "turn_a")
        .await
        .expect("list turn events");
    let event_ids: Vec<_> = event_rows.iter().map(|row| row.event_id.as_str()).collect();
    assert_eq!(event_ids, vec!["evt_b", "evt_a"]);
}
