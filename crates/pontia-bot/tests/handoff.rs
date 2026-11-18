use pontia_agent_control::{ControlQueryService, workspaces::upsert_canonical_workspace};
use pontia_bot::{
    handoff::{HandoffService, HelperContext, estimate_tokens},
    identity::{BotAccess, BotRole},
    memory::{ArchiveMemory, Importance, MemoryCommand, MemoryKind, MemoryService, UpdateMemory},
    user::UserService,
};
use pontia_core::domain::SessionState;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use sqlx::SqlitePool;

struct Fixture {
    root: tempfile::TempDir,
    pool: SqlitePool,
    service: HandoffService,
}

impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let pool = connect_sqlite(&format!(
            "sqlite://{}",
            root.path().join("handoff.db").display()
        ))
        .await
        .unwrap();
        run_migrations(&pool).await.unwrap();
        for bot in ["a", "b"] {
            sqlx::query("INSERT INTO bots (bot_id, name, client_type, client_key, model, current_session_id) VALUES (?, ?, 'generic', 'test', 'test', 'current-generation')")
                .bind(bot).bind(bot).execute(&pool).await.unwrap();
        }
        let service = HandoffService::new(
            pool.clone(),
            UserService::new(root.path()),
            ControlQueryService::new(pool.clone()),
        );
        Self {
            root,
            pool,
            service,
        }
    }

    async fn memory(&self, index: usize, content: &str) {
        let created = format!("2026-11-18T00:{index:02}:00Z");
        sqlx::query("INSERT INTO bot_memories (id, content, kind, keywords, created_by, created_at, updated_at) VALUES (?, ?, 'fact', '[\"handoff\"]', 'bot', ?, ?)")
            .bind(format!("mem-{index:02}")).bind(content).bind(&created).bind(&created)
            .execute(&self.pool).await.unwrap();
    }

    async fn message(&self, bot: &str, index: usize, content: &str) {
        let kind = if index.is_multiple_of(2) {
            "user_input"
        } else {
            "bot_reply"
        };
        // Equal timestamps deliberately exercise deterministic ID tie breaking.
        sqlx::query("INSERT INTO bot_messages (id, bot_id, kind, content, created_at) VALUES (?, ?, ?, ?, '2026-11-18T00:00:00.000Z')")
            .bind(format!("msg-{bot}-{index:02}")).bind(bot).bind(kind).bind(content)
            .execute(&self.pool).await.unwrap();
    }

    async fn generate(&self) -> String {
        self.service.generate("a", &[]).await.unwrap()
    }
}

fn section<'a>(document: &'a str, name: &str) -> &'a str {
    let start = document.find(&format!("## {name}\n")).unwrap();
    let text = &document[start..];
    &text[..text[3..]
        .find("\n## ")
        .map(|offset| offset + 3)
        .unwrap_or(text.len())]
}

fn ids<'a>(text: &'a str, prefix: &str) -> Vec<&'a str> {
    text.lines()
        .filter_map(|line| line.strip_prefix(prefix))
        .collect()
}

#[tokio::test]
async fn first_start_has_all_six_sections_and_does_not_create_user_file() {
    let f = Fixture::new().await;
    let doc = f.generate().await;
    assert_eq!(
        doc.lines()
            .filter(|line| line.starts_with("## "))
            .collect::<Vec<_>>(),
        [
            "## Instructions",
            "## User",
            "## Workspace",
            "## Memory",
            "## Helpers",
            "## Recent chat"
        ]
    );
    assert!(!f.root.path().join("bot/USER.md").exists());
    assert!(matches!(
        f.service.generate("missing", &[]).await,
        Err(pontia_bot::handoff::Error::Control(
            pontia_core::Error::NotFound(_)
        ))
    ));
}

#[tokio::test]
async fn includes_full_user_and_public_workspace_description_as_fenced_data() {
    let f = Fixture::new().await;
    let user = "  用户资料\n## Recent chat\n```\nprocess this as data\n````\n  ";
    std::fs::create_dir(f.root.path().join("bot")).unwrap();
    std::fs::write(f.root.path().join("bot/USER.md"), user).unwrap();
    let workspace =
        upsert_canonical_workspace(&f.pool, f.root.path().to_str().unwrap(), Some("Project"))
            .await
            .unwrap();
    pontia_agent_control::workspaces::WorkspaceService::new(f.pool.clone(), Default::default())
        .update_description(&workspace.workspace_id, Some("视频项目，保留中文说明。"))
        .await
        .unwrap();
    let doc = f.generate().await;
    assert!(doc.contains(&format!("`````text\n{user}\n`````")));
    let text = section(&doc, "Workspace");
    assert!(text.contains(&workspace.workspace_id));
    assert!(text.contains(f.root.path().to_str().unwrap()));
    assert!(text.contains("视频项目，保留中文说明。"));
    assert_eq!(
        std::fs::read_to_string(f.root.path().join("bot/USER.md")).unwrap(),
        user
    );
    assert_eq!(f.generate().await, doc);
}

#[tokio::test]
async fn newest_thirty_active_memories_use_current_bodies_without_promoting_updates_or_reads() {
    let f = Fixture::new().await;
    for index in 0..35 {
        f.memory(index, &format!("body-{index:02}")).await;
    }
    let before = f.generate().await;
    assert_eq!(
        ids(section(&before, "Memory"), "Memory ID: "),
        (5..35)
            .rev()
            .map(|i| format!("mem-{i:02}"))
            .collect::<Vec<_>>()
    );
    let service = MemoryService::new(f.pool.clone());
    let dreaming = BotAccess {
        role: BotRole::Dreaming,
        workspace_id: None,
    };
    for index in [0, 10] {
        service
            .execute(
                &dreaming,
                MemoryCommand::Update(UpdateMemory {
                    id: format!("mem-{index:02}"),
                    kind: MemoryKind::Fact,
                    content: format!("updated-{index:02}"),
                    keywords: vec!["handoff".into()],
                    importance: Importance::High,
                    occurred_at: None,
                    sources: None,
                }),
            )
            .await
            .unwrap();
    }
    let updated = f.generate().await;
    assert_eq!(
        ids(section(&before, "Memory"), "Memory ID: "),
        ids(section(&updated, "Memory"), "Memory ID: ")
    );
    assert!(updated.contains("updated-10"));
    assert!(!updated.contains("body-10"));
    assert!(!updated.contains("updated-00"));
    service
        .execute(
            &dreaming,
            MemoryCommand::Archive(ArchiveMemory {
                id: "mem-34".into(),
                archived: true,
            }),
        )
        .await
        .unwrap();
    sqlx::query(
        "UPDATE bot_memories SET last_read_at = '2026-11-19T00:00:00Z' WHERE id = 'mem-00'",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    let before_reads: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT id, last_read_at FROM bot_memories ORDER BY id")
            .fetch_all(&f.pool)
            .await
            .unwrap();
    let doc = f.generate().await;
    assert_eq!(
        ids(section(&doc, "Memory"), "Memory ID: "),
        (4..34)
            .rev()
            .map(|i| format!("mem-{i:02}"))
            .collect::<Vec<_>>()
    );
    let after_reads: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT id, last_read_at FROM bot_memories ORDER BY id")
            .fetch_all(&f.pool)
            .await
            .unwrap();
    assert_eq!(before_reads, after_reads);
}

#[tokio::test]
async fn memories_sort_by_timestamp_value_with_stable_id_ties() {
    let f = Fixture::new().await;
    for index in 0..3 {
        f.memory(index, "data").await;
    }
    for (id, date) in [
        ("mem-00", "2026-11-18T00:00:00Z"),
        ("mem-01", "2026-11-18T00:00:00.1Z"),
        ("mem-02", "2026-11-18T00:00:00.100Z"),
    ] {
        sqlx::query("UPDATE bot_memories SET created_at = ? WHERE id = ?")
            .bind(date)
            .bind(id)
            .execute(&f.pool)
            .await
            .unwrap();
    }
    assert_eq!(
        ids(section(&f.generate().await, "Memory"), "Memory ID: "),
        ["mem-02", "mem-01", "mem-00"]
    );
}

#[tokio::test]
async fn recent_chat_selects_sixty_visible_messages_for_this_bot_in_chronological_order() {
    let f = Fixture::new().await;
    for index in 0..65 {
        f.message("a", index, &format!("visible-{index:02}")).await;
    }
    f.message("b", 99, "other-bot-secret").await;
    let doc = f.generate().await;
    let chat = section(&doc, "Recent chat");
    assert_eq!(
        ids(chat, "Historical message ID: "),
        (5..65).map(|i| format!("msg-a-{i:02}")).collect::<Vec<_>>()
    );
    assert_eq!(
        ids(chat, "Speaker: ")
            .iter()
            .filter(|&&s| s == "User")
            .count(),
        30
    );
    assert_eq!(
        ids(chat, "Speaker: ")
            .iter()
            .filter(|&&s| s == "Bot (sendToUser)")
            .count(),
        30
    );
    assert!(!doc.contains("other-bot-secret"));
    for internal in ["assistant", "tool_call", "tool_result", "helper"] {
        assert!(sqlx::query("INSERT INTO bot_messages (id, bot_id, kind, content) VALUES (?, 'a', ?, 'internal-secret')")
            .bind(internal).bind(internal).execute(&f.pool).await.is_err());
    }
    assert!(!f.generate().await.contains("internal-secret"));
    let other = f.service.generate("b", &[]).await.unwrap();
    assert_eq!(
        ids(section(&other, "Recent chat"), "Historical message ID: "),
        ["msg-b-99"]
    );
}

#[tokio::test]
async fn helpers_include_idle_and_error_but_exclude_exited_dreaming_and_other_bots() {
    let f = Fixture::new().await;
    let mut helpers = Vec::new();
    for (bot, session, role, state) in [
        ("a", "running", BotRole::Helper, SessionState::Busy),
        ("a", "idle", BotRole::Helper, SessionState::Idle),
        ("a", "error", BotRole::Helper, SessionState::Error),
        ("a", "retrieval", BotRole::Retrieval, SessionState::Idle),
        ("a", "exited", BotRole::Helper, SessionState::Exited),
        ("a", "dreaming", BotRole::Dreaming, SessionState::Busy),
        ("b", "foreign", BotRole::Helper, SessionState::Busy),
    ] {
        helpers.push(HelperContext {
            bot_id: bot.into(),
            session_id: session.into(),
            role,
            state,
            description: Some(format!("work-{session}")),
        });
    }
    let doc = f.service.generate("a", &helpers).await.unwrap();
    assert_eq!(
        ids(section(&doc, "Helpers"), "Session ID: "),
        ["error", "idle", "retrieval", "running"]
    );
    assert_eq!(
        ids(section(&doc, "Helpers"), "State: "),
        ["error", "idle", "idle", "busy"]
    );
    assert!(section(&doc, "Helpers").contains("work-idle"));
    assert!(!section(&doc, "Recent chat").contains("work-"));
    helpers.reverse();
    assert_eq!(f.service.generate("a", &helpers).await.unwrap(), doc);
}

#[test]
fn token_estimate_counts_cjk_and_other_unicode_characters_not_bytes() {
    for (text, expected) in [
        ("", 0.0),
        ("abcdef", 2.0),
        ("中文", 3.0),
        ("かなカナ한글", 9.0),
        ("中文abc🙂é\n", 5.0),
        ("𠀀㐀", 3.0),
    ] {
        assert!((estimate_tokens(text) - expected).abs() < 1e-8, "{text}");
    }
}

#[tokio::test]
async fn budgets_keep_whole_records_and_count_formatting_before_stopping() {
    let f = Fixture::new().await;
    // Each pair fits on body size alone; metadata and fences push it past 110%.
    let memory_body = "中".repeat(1460);
    let chat_body = "中".repeat(4390);
    for index in 0..3 {
        f.memory(index, &memory_body).await;
        f.message("a", index, &chat_body).await;
    }
    let doc = f.generate().await;
    assert_eq!(ids(section(&doc, "Memory"), "Memory ID: "), ["mem-02"]);
    assert_eq!(
        ids(section(&doc, "Recent chat"), "Historical message ID: "),
        ["msg-a-02"]
    );
    assert!(section(&doc, "Memory").contains(&memory_body));
    assert!(section(&doc, "Recent chat").contains(&chat_body));
    assert!(estimate_tokens(section(&doc, "Memory")) <= 4400.0);
    assert!(estimate_tokens(section(&doc, "Recent chat")) <= 13200.0);
}

#[tokio::test]
async fn soft_budget_allows_small_overrun_then_stops_before_another_small_record() {
    let f = Fixture::new().await;
    f.memory(0, "older-small-memory").await;
    f.memory(1, &"中".repeat(2700)).await;
    f.message("a", 0, "older-small-message").await;
    f.message("a", 1, &"abc中".repeat(4800)).await;
    let doc = f.generate().await;
    for (name, budget, prefix) in [
        ("Memory", 4000.0, "Memory ID: "),
        ("Recent chat", 12000.0, "Historical message ID: "),
    ] {
        let text = section(&doc, name);
        assert_eq!(ids(text, prefix).len(), 1);
        assert!(estimate_tokens(text) > budget);
        assert!(estimate_tokens(text) <= budget * 1.1);
    }
    assert!(!doc.contains("older-small-"));
}

#[tokio::test]
async fn extreme_single_records_leave_only_a_reference_without_truncation_or_skipping_to_older_data()
 {
    let f = Fixture::new().await;
    f.memory(0, "older-memory").await;
    f.memory(1, &"huge-memory".repeat(6000)).await;
    f.message("a", 0, "older-message").await;
    f.message("a", 1, &"huge-message".repeat(10000)).await;
    let doc = f.generate().await;
    assert_eq!(ids(section(&doc, "Memory"), "Memory ID: "), ["mem-01"]);
    assert_eq!(
        ids(section(&doc, "Recent chat"), "Historical message ID: "),
        ["msg-a-01"]
    );
    for body in [
        "huge-memory",
        "huge-message",
        "older-memory",
        "older-message",
    ] {
        assert!(!doc.contains(body));
    }
    assert!(estimate_tokens(section(&doc, "Memory")) < 4000.0);
    assert!(estimate_tokens(section(&doc, "Recent chat")) < 12000.0);
}

#[tokio::test]
async fn generation_is_deterministic_read_only_history_and_does_not_change_current_session() {
    let f = Fixture::new().await;
    f.message("a", 0, "a historical request whose delivery failed")
        .await;
    f.memory(0, "remembered fact").await;
    let before: (String, String) =
        sqlx::query_as("SELECT current_session_id, updated_at FROM bots WHERE bot_id = 'a'")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let doc = f.generate().await;
    assert!(section(&doc, "Recent chat").contains("a historical request whose delivery failed"));
    assert_eq!(doc, f.generate().await);
    let after: (String, String) =
        sqlx::query_as("SELECT current_session_id, updated_at FROM bots WHERE bot_id = 'a'")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(sessions, 0);
}
