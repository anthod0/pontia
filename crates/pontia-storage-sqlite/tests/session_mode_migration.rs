use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use std::borrow::Cow;

#[tokio::test]
async fn upgrade_maps_existing_launches_and_preserves_native_fork_inheritance() {
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    let old = sqlx::migrate::Migrator {
        migrations: Cow::Owned(
            sqlx::migrate!("./migrations")
                .iter()
                .filter(|migration| migration.version < 68)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    old.run(&pool).await.unwrap();
    let cases = [
        (
            "old-bot",
            Some("bot"),
            json!({"execution_policy":{"disabled_tools":["spawn_agent"],"yolo":true}}),
            Some("restricted"),
        ),
        (
            "reader",
            Some("bot"),
            json!({"execution_policy":{"tools":["read","shell","web"],"disable_skills":true,"yolo":true}}),
            Some("restricted"),
        ),
        (
            "helper:workspace",
            Some("bot"),
            json!({"execution_policy":{"tools":null,"disabled_tools":[],"yolo":false}}),
            Some("default"),
        ),
        (
            "external",
            None,
            json!({"execution_policy":{"yolo":true}}),
            Some("yolo"),
        ),
        (
            "dreaming:legacy",
            Some("bot"),
            json!({}),
            Some("restricted"),
        ),
        (
            "helper:legacy",
            Some("bot"),
            json!({"execution_policy":{"disabled_tools":["spawn_agent"],"yolo":true}}),
            Some("restricted"),
        ),
        ("fork", None, json!({}), None),
    ];
    for (session, owner, payload, _) in &cases {
        sqlx::query("INSERT INTO sessions(session_id, client_type, state, application_id, application_ref) VALUES (?, 'pi', 'exited', ?, ?)")
            .bind(session).bind(owner).bind(owner.map(|_| session)).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO events(event_id,session_id,client_type,source,event_type,payload,occurred_at) VALUES(?,?,'pi','external_api','session.created',?,'2026-11-23T00:00:00Z')")
            .bind(session).bind(session).bind(payload.to_string()).execute(&pool).await.unwrap();
    }
    run_migrations(&pool).await.unwrap();
    for (session, _, _, mode) in cases {
        let payload: String = sqlx::query_scalar("SELECT payload FROM events WHERE event_id = ?")
            .bind(session)
            .fetch_one(&pool)
            .await
            .unwrap();
        let payload: Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(payload.get("mode").and_then(Value::as_str), mode);
        assert!(payload.get("execution_policy").is_none());
    }
}
