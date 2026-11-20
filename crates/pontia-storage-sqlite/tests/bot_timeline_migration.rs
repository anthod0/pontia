use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use std::borrow::Cow;

#[tokio::test]
async fn existing_messages_and_session_events_page_by_truncated_utc_milliseconds_after_upgrade() {
    let root = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", root.path().join("upgrade.db").display());
    let pool = connect_sqlite(&url).await.unwrap();
    let old = sqlx::migrate::Migrator {
        migrations: Cow::Owned(
            sqlx::migrate!("./migrations")
                .iter()
                .filter(|m| m.version < 50)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    old.run(&pool).await.unwrap();
    sqlx::query("INSERT INTO bots(bot_id,name,client_type,client_key,model) VALUES('bot','Test','generic','test','test')").execute(&pool).await.unwrap();
    for (id, time) in [
        ("B", "1970-01-01T08:00:00.999999999+08:00"),
        ("A", "1969-12-31T22:30:00.9999-01:30"),
        ("last", "1970-01-01T00:00:01Z"),
        ("first", "1969-12-31T23:59:59.9999Z"),
    ] {
        sqlx::query("INSERT INTO bot_messages(bot_id,id,kind,content,created_at) VALUES('bot',?,'user_input',?,?)")
            .bind(id).bind(id).bind(time).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO events(event_id,session_id,client_type,source,event_type,occurred_at) VALUES(?,'session','generic','agent_adapter','session.error',?)")
            .bind(id).bind(time).execute(&pool).await.unwrap();
    }
    pool.close().await;
    let pool = connect_sqlite(&url).await.unwrap();
    run_migrations(&pool).await.unwrap();
    pool.close().await;
    let reopened = connect_sqlite(&url).await.unwrap();
    for query in [
        "SELECT id, timeline_ms FROM bot_messages ORDER BY timeline_ms, id",
        "SELECT event_id, timeline_ms FROM events ORDER BY timeline_ms, event_id",
    ] {
        let actual: Vec<(String, i64)> = sqlx::query_as(query).fetch_all(&reopened).await.unwrap();
        assert_eq!(
            actual,
            [
                ("first".into(), -1),
                ("A".into(), 999),
                ("B".into(), 999),
                ("last".into(), 1000)
            ]
        );
    }
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT occurred_at FROM events WHERE event_id='A'")
            .fetch_one(&reopened)
            .await
            .unwrap(),
        "1969-12-31T22:30:00.9999-01:30"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
            .fetch_one(&reopened)
            .await
            .unwrap(),
        "ok"
    );
}
