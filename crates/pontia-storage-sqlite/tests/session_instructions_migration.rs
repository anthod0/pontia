use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use std::borrow::Cow;

#[tokio::test]
async fn session_profile_columns_are_removed_on_fresh_install_and_upgrade() {
    for upgrade in [false, true] {
        let pool = connect_sqlite("sqlite::memory:").await.unwrap();
        if upgrade {
            let old = sqlx::migrate::Migrator {
                migrations: Cow::Owned(
                    sqlx::migrate!("./migrations")
                        .iter()
                        .filter(|migration| migration.version < 52)
                        .cloned()
                        .collect(),
                ),
                ..sqlx::migrate::Migrator::DEFAULT
            };
            old.run(&pool).await.unwrap();
            sqlx::query("INSERT INTO sessions(session_id,client_type,state,title,execution_profile_id,execution_profile_version) VALUES('existing','pi','created','Keep title','reviewer','2')")
                .execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO events(event_id,session_id,client_type,source,event_type,payload,occurred_at) VALUES('created','existing','pi','external_api','session.created','{\"workspace\":\"/project\"}','2026-11-20T00:00:00Z')")
                .execute(&pool).await.unwrap();
        }
        run_migrations(&pool).await.unwrap();
        let columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('sessions')")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert!(
            !columns
                .iter()
                .any(|column| column.starts_with("execution_profile_"))
        );
        let index: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sqlite_schema WHERE name='idx_sessions_execution_profile'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(index, 0);
        if upgrade {
            let session: (String, String) =
                sqlx::query_as("SELECT title,state FROM sessions WHERE session_id='existing'")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(session, ("Keep title".into(), "created".into()));
            let payload: String =
                sqlx::query_scalar("SELECT payload FROM events WHERE event_id='created'")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(payload, r#"{"workspace":"/project"}"#);
        }
        assert_eq!(
            sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
                .fetch_one(&pool)
                .await
                .unwrap(),
            "ok"
        );
        pool.close().await;
    }
}
