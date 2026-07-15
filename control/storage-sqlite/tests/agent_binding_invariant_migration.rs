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
    migrator_through(36)
        .run(&pool)
        .await
        .expect("migrate through 0036");
    pool
}

async fn insert_session(pool: &sqlx::SqlitePool, session_id: &str) {
    sqlx::query("INSERT INTO sessions (session_id, client_type, state) VALUES (?, 'pi', 'ready')")
        .bind(session_id)
        .execute(pool)
        .await
        .expect("insert session");
}

#[tokio::test]
async fn migration_rejects_sessions_with_multiple_agent_bindings() {
    let pool = historical_pool("duplicate-session-bindings.db").await;
    insert_session(&pool, "sess_duplicate").await;
    sqlx::query(
        r#"INSERT INTO agent_bindings
           (id, session_id, client_type, launch_cwd, client_session_key)
           VALUES
             ('bind_1', 'sess_duplicate', 'pi', '/repo', 'pi_one'),
             ('bind_2', 'sess_duplicate', 'pi', '/repo', 'pi_two')"#,
    )
    .execute(&pool)
    .await
    .expect("seed duplicate bindings");

    let error = MIGRATOR.run(&pool).await.expect_err("migration must fail");
    assert!(
        error
            .to_string()
            .contains("migration 0037 found multiple Agent bindings for one Session"),
        "{error}"
    );
}

#[tokio::test]
async fn migration_rejects_client_identity_bound_to_multiple_sessions() {
    let pool = historical_pool("duplicate-client-identity.db").await;
    insert_session(&pool, "sess_one").await;
    insert_session(&pool, "sess_two").await;
    sqlx::query(
        r#"INSERT INTO agent_bindings
           (id, session_id, client_type, launch_cwd, client_session_key)
           VALUES
             ('bind_1', 'sess_one', 'pi', '/repo', 'pi_shared'),
             ('bind_2', 'sess_two', 'pi', '/repo', 'pi_shared')"#,
    )
    .execute(&pool)
    .await
    .expect("seed duplicate identity");

    let error = MIGRATOR.run(&pool).await.expect_err("migration must fail");
    assert!(
        error
            .to_string()
            .contains("migration 0037 found one client identity bound to multiple Sessions"),
        "{error}"
    );
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
