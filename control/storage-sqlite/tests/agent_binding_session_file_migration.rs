use pontia_storage_sqlite::connect_sqlite;

#[tokio::test]
async fn migration_promotes_client_session_file_from_agent_binding_metadata() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("agent-binding-session-file-migration.db");
    let pool = connect_sqlite(&format!("sqlite://{}", db_path.display()))
        .await
        .expect("connect");

    sqlx::raw_sql(include_str!("../migrations/0001_init.sql"))
        .execute(&pool)
        .await
        .expect("initialize baseline schema");
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state, metadata)
           VALUES ('sess_with_file', 'pi', 'ready', '{}'),
                  ('sess_without_file', 'pi', 'ready', '{}')"#,
    )
    .execute(&pool)
    .await
    .expect("insert sessions");
    sqlx::query(
        r#"INSERT INTO agent_bindings
           (id, session_id, client_type, launch_cwd, client_session_key, metadata)
           VALUES ('bind_with_file', 'sess_with_file', 'pi', '/workspace', 'key-with-file',
                   '{"client_session_file":"/tmp/pi/session.jsonl","diagnostic":true}'),
                  ('bind_without_file', 'sess_without_file', 'pi', '/workspace', 'key-without-file',
                   '{"diagnostic":true}')"#,
    )
    .execute(&pool)
    .await
    .expect("insert legacy bindings");

    sqlx::raw_sql(include_str!(
        "../migrations/0009_add_agent_binding_client_session_file.sql"
    ))
    .execute(&pool)
    .await
    .expect("promote client session file");

    let promoted: Option<String> = sqlx::query_scalar(
        "SELECT client_session_file FROM agent_bindings WHERE id = 'bind_with_file'",
    )
    .fetch_one(&pool)
    .await
    .expect("load promoted path");
    let absent: Option<String> = sqlx::query_scalar(
        "SELECT client_session_file FROM agent_bindings WHERE id = 'bind_without_file'",
    )
    .fetch_one(&pool)
    .await
    .expect("load absent path");

    assert_eq!(promoted.as_deref(), Some("/tmp/pi/session.jsonl"));
    assert_eq!(absent, None);
}
