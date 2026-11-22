use super::*;

async fn close_helper(control: &AgentControl, session: &str) {
    control
        .session_commands()
        .terminate_session(session)
        .await
        .unwrap();
    let runtime = control
        .queries()
        .session_snapshot(session)
        .await
        .unwrap()
        .unwrap()
        .runtimes[0]
        .runtime_id
        .clone();
    control
        .event_ingest_service()
        .report_client_fact(
            session,
            Some(&runtime),
            EventType::SessionExited,
            json!({"reason":"test client exit"}),
        )
        .await
        .unwrap();
}

async fn count(pool: &sqlx::SqlitePool, table: &str, bot_id: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM {table} WHERE bot_id = ?"))
        .bind(bot_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn assert_blocked(error: Error) {
    assert!(
        matches!(
            error,
            Error::Conflict {
                code: "bot_delete_blocked",
                ..
            }
        ),
        "{error:?}"
    );
}

#[tokio::test]
async fn open_helpers_block_deletion_until_closed_then_only_that_bot_is_removed() {
    let (root, pool, _client, control, conversation) = fixture().await;
    let _exits = report_exits(&control);
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let (doomed, session) = bot(&conversation).await;
    let (kept, _) = bot(&conversation).await;
    let worker = helper(&helpers, &session).await;
    finish(&control, &worker, EventType::TurnCompleted).await;
    finish(&control, &session, EventType::TurnCompleted).await;
    sqlx::query("INSERT INTO bot_notifications(id, bot_id, content) VALUES ('n', ?, 'pending')")
        .bind(&doomed)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO bot_memories(id, kind, content, keywords, session_id, created_by)
         VALUES ('m', 'fact', 'shared', '[\"shared\"]', ?, 'bot')",
    )
    .bind(&session)
    .execute(&pool)
    .await
    .unwrap();
    let user = root.path().join("bot/USER.md");
    std::fs::write(&user, "user notes").unwrap();
    let directory = root.path().join("bot").join(&doomed);
    assert!(directory.is_dir());

    let blockers = conversation.deletion_blockers(&doomed).await.unwrap();
    assert!(!blockers.running);
    assert_eq!(
        blockers
            .helpers
            .iter()
            .map(|helper| helper.session_id.as_str())
            .collect::<Vec<_>>(),
        [worker.as_str()]
    );
    assert_blocked(conversation.delete(&doomed).await.unwrap_err());
    assert!(conversation.records().get(&doomed).await.is_ok());
    assert!(directory.is_dir());
    assert_eq!(
        control
            .queries()
            .session_snapshot(&worker)
            .await
            .unwrap()
            .unwrap()
            .state,
        "idle",
        "a rejected deletion must not stop helpers"
    );

    close_helper(&control, &worker).await;
    conversation.delete(&doomed).await.unwrap();

    assert!(matches!(
        conversation.records().get(&doomed).await,
        Err(Error::NotFound(_))
    ));
    for table in ["bot_messages", "bot_helpers", "bot_notifications"] {
        assert_eq!(count(&pool, table, &doomed).await, 0, "{table}");
    }
    assert!(!directory.exists());
    assert_eq!(
        control
            .queries()
            .session_snapshot(&session)
            .await
            .unwrap()
            .unwrap()
            .state,
        "exited",
        "the main Session record remains after its Bot is gone"
    );
    assert!(conversation.resolve(&session).await.is_err());
    assert!(conversation.resolve(&worker).await.is_err());

    assert!(conversation.records().get(&kept).await.is_ok());
    assert_eq!(count(&pool, "bot_messages", &kept).await, 2);
    assert!(root.path().join("bot").join(&kept).is_dir());
    assert_eq!(std::fs::read_to_string(&user).unwrap(), "user notes");
    let memories: i64 = sqlx::query_scalar("SELECT count(*) FROM bot_memories WHERE id = 'm'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(memories, 1);
    assert!(
        !control
            .queries()
            .list_workspaces()
            .await
            .unwrap()
            .is_empty(),
        "Workspaces stay shared"
    );
}

#[tokio::test]
async fn a_working_bot_cannot_be_deleted() {
    let (_root, _pool, _client, control, conversation) = fixture().await;
    let (id, session) = bot(&conversation).await;
    super::status::start(&control, &session).await;
    assert!(conversation.deletion_blockers(&id).await.unwrap().running);
    assert_blocked(conversation.delete(&id).await.unwrap_err());
    assert!(conversation.records().get(&id).await.is_ok());
    assert_ne!(
        control
            .queries()
            .session_snapshot(&session)
            .await
            .unwrap()
            .unwrap()
            .state,
        "exited"
    );
}

#[tokio::test]
async fn the_last_bot_can_be_deleted_and_a_new_one_starts_cleanly() {
    let (_root, _pool, _client, control, conversation) = fixture().await;
    let _exits = report_exits(&control);
    let (id, session) = bot(&conversation).await;
    finish(&control, &session, EventType::TurnCompleted).await;
    conversation.delete(&id).await.unwrap();
    assert!(conversation.records().list().await.unwrap().is_empty());
    let (next, next_session) = bot(&conversation).await;
    assert_ne!(next, id);
    assert_ne!(next_session, session);
}
