use pontia_storage_sqlite::{
    connect_sqlite,
    repositories::event_subscriptions::{EventScope, EventSubscriptionRepository},
    run_migrations,
};
use std::num::NonZeroU32;

#[tokio::test]
async fn migration_seeds_existing_cursors_and_registration_preserves_progress() {
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    let migrations = sqlx::migrate!("./migrations");
    for migration in migrations.iter().take_while(|m| m.version < 79) {
        sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
    }
    sqlx::raw_sql(
        "UPDATE workflow_event_cursor SET position=9; UPDATE bot_event_cursor SET position=3",
    )
    .execute(&pool)
    .await
    .unwrap();
    for migration in migrations.iter().filter(|m| m.version >= 79) {
        sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
    }
    let store = EventSubscriptionRepository::new(pool.clone());
    for (id, pos) in [("workflow.projection", 9), ("bot.helper_notifications", 3)] {
        store.register(id).await.unwrap();
        store.advance(id, 1).await.unwrap();
        assert_eq!(store.cursor(id).await.unwrap(), pos);
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sqlite_schema WHERE name IN ('workflow_event_cursor','bot_event_cursor')")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn scan_bounds_global_positions_before_filtering_and_resolves_in_scope() {
    let db = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&db).await.unwrap();
    sqlx::query("INSERT INTO sessions(session_id,client_type,state,application_id,application_ref) VALUES('owned','generic','idle','bot','helper')").execute(&db).await.unwrap();
    for (id, session) in [("a", "missing"), ("b", "missing"), ("c", "owned")] {
        sqlx::query("INSERT INTO events(event_id,session_id,turn_id,source,client_type,event_type,occurred_at) VALUES(?,?,'t','external_api','generic','turn.failed','2026-01-01T00:00:00Z')")
            .bind(id).bind(session).execute(&db).await.unwrap();
    }
    let store = EventSubscriptionRepository::new(db);
    let head = store.current_head_position().await.unwrap();
    assert_eq!(head, 3);
    let scope = EventScope::Application("bot".into());
    let size = NonZeroU32::new(2).unwrap();
    let first = store.scan(&scope, &[], 0, head, size).await.unwrap();
    assert!(first.events.is_empty());
    assert_eq!(first.end_position, 2);
    let second = store
        .scan(&scope, &[], first.end_position, head, size)
        .await
        .unwrap();
    assert_eq!(second.end_position, 3);
    let [event] = second.events.as_slice() else {
        panic!("expected owned event")
    };
    assert_eq!(event.application_id.as_deref(), Some("bot"));
    assert_eq!(event.application_ref.as_deref(), Some("helper"));
    assert_eq!(event.client_type, "generic");
    let orphans = store
        .scan(&EventScope::All, &[], 0, head, size)
        .await
        .unwrap();
    assert_eq!(orphans.events.len(), 2);
    assert_eq!(orphans.events[0].application_id, None);
    assert_eq!(
        store.resolve_event_position(&scope, "a").await.unwrap(),
        None
    );
    assert_eq!(
        store
            .resolve_event_position(
                &EventScope::Turn {
                    session_id: "owned".into(),
                    turn_id: "t".into()
                },
                "c"
            )
            .await
            .unwrap(),
        Some(3)
    );
    let filtered = store
        .scan(&scope, &["session.error".into()], 2, head, size)
        .await
        .unwrap();
    assert!(filtered.events.is_empty());
    assert_eq!(filtered.end_position, 3);
    let types = ["turn.failed".into()];
    assert!(
        store
            .has_session_event_after("owned", 2, &types)
            .await
            .unwrap()
    );
    assert!(
        !store
            .has_session_event_after("owned", 3, &types)
            .await
            .unwrap()
    );
    assert!(
        !store
            .has_session_event_after("missing", 2, &types)
            .await
            .unwrap()
    );
    assert!(
        !store
            .has_session_event_after("owned", 0, &["session.error".into()])
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn missing_legacy_cursor_starts_at_zero_on_registration() {
    let db = connect_sqlite("sqlite::memory:").await.unwrap();
    sqlx::raw_sql("CREATE TABLE bot_event_cursor(id INTEGER PRIMARY KEY, position INTEGER); INSERT INTO bot_event_cursor VALUES (1,7)").execute(&db).await.unwrap();
    sqlx::raw_sql(include_str!("../migrations/0079_event_subscriptions.sql"))
        .execute(&db)
        .await
        .unwrap();
    let store = EventSubscriptionRepository::new(db);
    store.register("workflow.projection").await.unwrap();
    assert_eq!(store.cursor("workflow.projection").await.unwrap(), 0);
    assert_eq!(store.cursor("bot.helper_notifications").await.unwrap(), 7);
}
