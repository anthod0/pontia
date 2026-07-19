use pontia_storage_sqlite::{connect_sqlite, run_migrations};

async fn insert_session(pool: &sqlx::SqlitePool, session_id: &str) {
    sqlx::query("INSERT INTO sessions (session_id, client_type, state) VALUES (?, 'pi', 'ready')")
        .bind(session_id)
        .execute(pool)
        .await
        .expect("insert session");
}

#[tokio::test]
async fn storage_enforces_one_to_one_agent_bindings() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("agent-binding-invariants.db");
    let pool = connect_sqlite(&format!("sqlite://{}", db_path.display()))
        .await
        .expect("connect");
    run_migrations(&pool).await.expect("migrate");
    insert_session(&pool, "sess_one").await;
    insert_session(&pool, "sess_two").await;
    sqlx::query(
        r#"INSERT INTO agent_bindings
           (id, session_id, client_type, launch_cwd, client_session_key)
           VALUES ('bind_1', 'sess_one', 'pi', '/repo', 'pi_one')"#,
    )
    .execute(&pool)
    .await
    .expect("insert first binding");

    let second_for_session = sqlx::query(
        r#"INSERT INTO agent_bindings
           (id, session_id, client_type, launch_cwd, client_session_key)
           VALUES ('bind_2', 'sess_one', 'pi', '/repo', 'pi_two')"#,
    )
    .execute(&pool)
    .await
    .expect_err("one Session cannot have two bindings");
    assert!(
        second_for_session
            .to_string()
            .contains("UNIQUE constraint failed")
    );

    let identity_rebound = sqlx::query(
        r#"INSERT INTO agent_bindings
           (id, session_id, client_type, launch_cwd, client_session_key)
           VALUES ('bind_3', 'sess_two', 'pi', '/repo', 'pi_one')"#,
    )
    .execute(&pool)
    .await
    .expect_err("one client identity cannot have two Sessions");
    assert!(
        identity_rebound
            .to_string()
            .contains("UNIQUE constraint failed")
    );
}
