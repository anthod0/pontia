use pontia_bot::identity::{BotAccess, BotIdentity, BotRole, IdentityError};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
};

use pontia_bot::memory::*;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use time::OffsetDateTime;

struct TestIdentity;
impl BotIdentity for TestIdentity {
    fn resolve<'a>(
        &'a self,
        session: &'a str,
    ) -> Pin<Box<dyn Future<Output = std::result::Result<BotAccess, IdentityError>> + Send + 'a>>
    {
        Box::pin(async move {
            let (role, workspace) = match session {
                "bot" => (BotRole::Bot, None),
                "dreaming" => (BotRole::Dreaming, None),
                "reader" => (BotRole::Retrieval, None),
                "reader-a" => (BotRole::Retrieval, Some("a")),
                "bot-a" => (BotRole::Bot, Some("a")),
                "dreaming-a" => (BotRole::Dreaming, Some("a")),
                _ => return Err(IdentityError),
            };
            Ok(BotAccess {
                role,
                workspace_id: workspace.map(str::to_owned),
            })
        })
    }
}

struct Fixture {
    commands: MemoryCommands,
    pool: sqlx::SqlitePool,
    seconds: Arc<AtomicI64>,
    _root: tempfile::TempDir,
}
impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let pool = connect_sqlite(&format!(
            "sqlite://{}",
            root.path().join("memory.db").display()
        ))
        .await
        .unwrap();
        run_migrations(&pool).await.unwrap();
        let seconds = Arc::new(AtomicI64::new(1_800_000_000));
        let clock = seconds.clone();
        let service = MemoryService::with_clock(pool.clone(), move || {
            OffsetDateTime::from_unix_timestamp(clock.load(Ordering::SeqCst)).unwrap()
        });
        Self {
            commands: MemoryCommands::new(service, Arc::new(TestIdentity)),
            pool,
            seconds,
            _root: root,
        }
    }
    fn days(&self, days: i64) {
        self.seconds
            .store(1_800_000_000 + days * 86_400, Ordering::SeqCst);
    }
    async fn call(&self, session: &str, action: &str, mut input: Value) -> Result<Value> {
        let reason = input
            .as_object_mut()
            .and_then(|input| input.remove("reason"))
            .unwrap_or(json!("Test evidence"));
        let request = serde_json::from_value(
            json!({"session_id": session, "reason":reason, "command": {"action": action, "input": input}}),
        )
        .unwrap();
        Ok(serde_json::to_value(self.commands.execute(request).await?).unwrap())
    }
    async fn create(&self, workspace: Option<&str>, words: &[&str]) -> Value {
        self.call("bot", "create", json!({"kind":"fact", "content":"  原始正文\nbody-only-secret  ", "keywords":words, "workspace_id":workspace, "session_id":"opaque-session", "sources":"用户先前的说明"})).await.unwrap()["memory"].clone()
    }
    async fn search(&self, words: &[&str], workspace: Option<&str>, limit: Option<usize>) -> Value {
        self.call(
            "bot",
            "search",
            json!({"keywords":words, "workspace_id":workspace, "limit":limit}),
        )
        .await
        .unwrap()
    }
    async fn read(&self, id: &Value) -> Value {
        self.call("reader", "read", json!({"ids":[id]}))
            .await
            .unwrap()["memories"][0]
            .clone()
    }
    async fn archive(&self, id: &Value, archived: bool) {
        self.call("dreaming", "archive", json!({"id":id,"archived":archived}))
            .await
            .unwrap();
    }
    async fn link(&self, a: &Value, b: &Value) -> Value {
        self.call("dreaming", "link", json!({"memory_a_id":a,"memory_b_id":b}))
            .await
            .unwrap()
    }
    async fn neighbors(&self, session: &str, id: &Value, workspace: Option<&str>) -> Value {
        self.call(
            session,
            "neighbors",
            json!({"id":id,"workspace_id":workspace}),
        )
        .await
        .unwrap()
    }
}

#[tokio::test]
async fn keywords_preserve_boundaries_and_never_execute_query_syntax() {
    let f = Fixture::new().await;
    let memory = f
        .create(
            None,
            &[
                " 中 ",
                "中文",
                "Agent  客户端",
                "C++",
                "Rust项目",
                "a OR b",
                "\" OR *",
                "中",
            ],
        )
        .await;
    assert_eq!(
        memory["keywords"],
        json!([
            "中",
            "中文",
            "agent 客户端",
            "c++",
            "rust项目",
            "a or b",
            "\" or *"
        ])
    );
    for query in [
        "中",
        "中文",
        " AGENT\t客户端 ",
        "c++",
        "RUST项目",
        "a or b",
        "\" OR *",
    ] {
        let found = f.search(&[query], None, None).await;
        assert_eq!(found["results"].as_array().unwrap().len(), 1, "{query}");
        assert_eq!(found["results"][0]["memory"]["id"], memory["id"]);
    }
    for query in [
        "body-only-secret",
        "文",
        "agent",
        "客户端",
        "rust",
        "c",
        "*",
        "OR",
        "NOT",
        "NEAR(中 中文)",
        "keywords:中",
    ] {
        assert_eq!(
            f.search(&[query], None, None).await["results"],
            json!([]),
            "{query}"
        );
    }
    assert_eq!(
        f.search(&["missing", "中"], None, None).await["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(f.read(&memory["id"]).await["last_read_at"], Value::Null);
}

#[tokio::test]
async fn rejects_invalid_input_without_writing_memories() {
    let f = Fixture::new().await;
    for keywords in [
        json!([]),
        json!([""]),
        json!([" \t\n"]),
        json!(["x\u{0000}y"]),
        json!(["x".repeat(129)]),
        json!(vec!["a"; 33]),
    ] {
        for action in ["create", "search"] {
            let input = if action == "create" {
                json!({"kind":"fact", "content":"valid", "keywords":keywords})
            } else {
                json!({"keywords": keywords})
            };
            assert!(matches!(
                f.call("bot", action, input).await,
                Err(Error::InvalidInput(_))
            ));
        }
    }
    for patch in [
        json!({"content":" \n\t"}),
        json!({"content":"x".repeat(65_537)}),
        json!({"content":"x\u{0000}"}),
        json!({"workspace_id":""}),
        json!({"session_id":""}),
        json!({"occurred_at":"yesterday"}),
        json!({"sources":"x".repeat(16_385)}),
    ] {
        let mut input = json!({"kind":"fact", "content":"valid", "keywords":["valid"]});
        input
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(matches!(
            f.call("bot", "create", input).await,
            Err(Error::InvalidInput(_))
        ));
    }
    for limit in [0, 31] {
        assert!(matches!(
            f.call("bot", "search", json!({"keywords":["x"],"limit":limit}))
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
    for ids in [json!([]), json!([""]), json!(vec!["id"; 101])] {
        assert!(matches!(
            f.call("reader", "read", json!({"ids":ids})).await,
            Err(Error::InvalidInput(_))
        ));
    }
    for input in [
        json!({"kind":"task", "content":"ok", "keywords":["x"]}),
        json!({"kind":"fact", "content":"ok", "keywords":[1]}),
        json!({"kind":"fact", "content":"ok", "keywords":["x"], "created_by":"dreaming"}),
    ] {
        assert!(
            serde_json::from_value::<MemoryRequest>(
                json!({"session_id":"bot","command":{"action":"create","input":input}})
            )
            .is_err()
        );
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM bot_memories")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    let long = "中".repeat(128);
    f.create(None, &[&long]).await;
    assert_eq!(
        f.search(&[&long], None, None).await["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn update_keeps_index_consistent_and_archive_hides_all_read_paths() {
    let f = Fixture::new().await;
    let a = f.create(Some("a"), &["before"]).await;
    let b = f.create(Some("a"), &["neighbor"]).await;
    f.link(&a["id"], &b["id"]).await;
    assert_eq!(a["content"], "  原始正文\nbody-only-secret  ");
    assert_eq!(a["importance"], "normal");
    assert_eq!(a["created_by"], "bot");
    assert_eq!(a["created_at"], a["updated_at"]);
    f.days(1);
    f.call("dreaming", "update", json!({"id":a["id"], "kind":"decision", "content":"  new body\n", "keywords":["AFTER"], "importance":"high", "occurred_at":"2026-11-01T08:00:00+08:00", "sources":"补充来源"})).await.unwrap();
    assert_eq!(
        f.search(&["before"], None, None).await["results"],
        json!([])
    );
    let updated = f.search(&["after"], None, None).await["results"][0]["memory"].clone();
    assert_eq!(updated["content"], "  new body\n");
    assert_eq!(updated["keywords"], json!(["after"]));
    assert_eq!(updated["kind"], "decision");
    assert_eq!(updated["occurred_at"], "2026-11-01T00:00:00Z");
    assert_eq!(updated["session_id"], "opaque-session");
    assert_eq!(updated["sources"], "补充来源");
    assert_eq!(updated["workspace_id"], "a");
    assert_eq!(updated["created_at"], a["created_at"]);
    assert_ne!(updated["updated_at"], a["updated_at"]);
    assert!(updated["last_read_at"].is_null());
    f.archive(&a["id"], true).await;
    assert_eq!(f.search(&["after"], None, None).await["results"], json!([]));
    assert!(f.read(&a["id"]).await.is_null());
    assert_eq!(
        f.neighbors("reader", &a["id"], None).await["memories"],
        json!([])
    );
    assert_eq!(
        f.neighbors("reader", &b["id"], None).await["memories"],
        json!([])
    );
    assert_eq!(
        f.call("bot", "use", json!({"ids":[a["id"]]}))
            .await
            .unwrap()["affected"],
        0
    );
    f.archive(&a["id"], false).await;
    assert_eq!(
        f.search(&["after"], None, None).await["results"][0]["memory"]["id"],
        a["id"]
    );
    assert_eq!(
        f.neighbors("reader", &b["id"], None).await["memories"][0]["id"],
        a["id"]
    );
}

#[tokio::test]
async fn graph_is_undirected_deduplicated_and_cannot_cross_read_scope() {
    let f = Fixture::new().await;
    let a = f.create(Some("a"), &["shared"]).await;
    let a2 = f.create(Some("a"), &["shared"]).await;
    let b = f.create(Some("b"), &["shared"]).await;
    let global = f.create(None, &["shared"]).await;
    for target in [&a2, &b, &global] {
        assert_eq!(f.link(&a["id"], &target["id"]).await["affected"], 1);
    }
    assert_eq!(f.link(&b["id"], &a["id"]).await["affected"], 0);
    assert!(matches!(
        f.call(
            "dreaming",
            "link",
            json!({"memory_a_id":a["id"], "memory_b_id":a["id"]})
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
    assert_eq!(
        f.neighbors("reader", &a["id"], None).await["memories"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        f.neighbors("reader", &b["id"], None).await["memories"][0]["id"],
        a["id"]
    );
    let scoped = f.neighbors("reader-a", &a["id"], None).await;
    assert_eq!(scoped["memories"].as_array().unwrap().len(), 1);
    assert_eq!(scoped["memories"][0]["id"], a2["id"]);
    assert_eq!(
        f.neighbors("reader-a", &b["id"], None).await["memories"],
        json!([])
    );
    assert_eq!(
        f.neighbors("reader", &b["id"], Some("a")).await["memories"],
        json!([])
    );
    assert_eq!(
        f.search(&["shared"], None, None).await["results"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        f.search(&["shared"], Some("a"), None).await["results"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let read = f
        .call(
            "reader-a",
            "read",
            json!({"ids":[a["id"],b["id"],global["id"]]}),
        )
        .await
        .unwrap();
    assert_eq!(read["memories"].as_array().unwrap().len(), 1);
    assert_eq!(read["memories"][0]["id"], a["id"]);
    for (action, input) in [
        ("search", json!({"keywords":["shared"],"workspace_id":"b"})),
        ("read", json!({"ids":[b["id"]],"workspace_id":"b"})),
        ("neighbors", json!({"id":b["id"],"workspace_id":"b"})),
    ] {
        assert!(matches!(
            f.call("reader-a", action, input).await,
            Err(Error::Forbidden)
        ));
    }
    let first = f
        .call("reader", "neighbors", json!({"id":a["id"],"limit":2}))
        .await
        .unwrap();
    assert_eq!(first["memories"].as_array().unwrap().len(), 2);
    assert!(first["next_cursor"].is_string());
    let second = f
        .call(
            "reader",
            "neighbors",
            json!({"id":a["id"],"limit":2,"after":first["next_cursor"]}),
        )
        .await
        .unwrap();
    assert_eq!(second["memories"].as_array().unwrap().len(), 1);
    assert!(second["next_cursor"].is_null());
    assert!(
        first["memories"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["id"] != second["memories"][0]["id"])
    );
    assert_eq!(
        f.call(
            "dreaming",
            "unlink",
            json!({"memory_a_id":b["id"],"memory_b_id":a["id"]})
        )
        .await
        .unwrap()["affected"],
        1
    );
    assert_eq!(
        f.neighbors("reader", &b["id"], None).await["memories"],
        json!([])
    );
}

#[tokio::test]
async fn only_assigned_roles_can_write_or_report_use() {
    let f = Fixture::new().await;
    let a = f.create(Some("a"), &["role"]).await;
    let b = f.create(Some("b"), &["role"]).await;
    let operations = [
        (
            "create",
            json!({"kind":"observation","content":"deduced","keywords":["role"]}),
            vec!["bot", "dreaming"],
        ),
        (
            "update",
            json!({"id":a["id"],"kind":"lesson","content":"changed","keywords":["role"],"importance":"normal"}),
            vec!["dreaming"],
        ),
        (
            "archive",
            json!({"id":a["id"],"archived":false}),
            vec!["dreaming"],
        ),
        (
            "link",
            json!({"memory_a_id":a["id"],"memory_b_id":b["id"]}),
            vec!["dreaming"],
        ),
        (
            "unlink",
            json!({"memory_a_id":a["id"],"memory_b_id":b["id"]}),
            vec!["dreaming"],
        ),
        ("use", json!({"ids":[a["id"]]}), vec!["bot"]),
        (
            "read",
            json!({"ids":[a["id"]]}),
            vec!["bot", "dreaming", "reader"],
        ),
        (
            "search",
            json!({"keywords":["role"]}),
            vec!["bot", "dreaming", "reader"],
        ),
        (
            "neighbors",
            json!({"id":a["id"]}),
            vec!["bot", "dreaming", "reader"],
        ),
    ];
    for (action, input, allowed) in operations {
        for session in ["bot", "dreaming", "reader", "unbound"] {
            let result = f.call(session, action, input.clone()).await;
            if allowed.contains(&session) {
                let result = result.unwrap();
                if action == "create" {
                    assert_eq!(result["memory"]["created_by"], session);
                }
            } else {
                assert!(
                    matches!(result, Err(Error::Forbidden)),
                    "{session} {action}: {result:?}"
                );
            }
        }
    }
    assert!(matches!(
        f.call(
            "bot-a",
            "create",
            json!({"kind":"fact","content":"x","keywords":["x"]})
        )
        .await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        f.call(
            "dreaming-a",
            "archive",
            json!({"id":b["id"],"archived":true})
        )
        .await,
        Err(Error::NotFound)
    ));
    assert!(matches!(
        f.call(
            "dreaming-a",
            "link",
            json!({"memory_a_id":a["id"],"memory_b_id":b["id"]})
        )
        .await,
        Err(Error::NotFound)
    ));
    assert_eq!(
        f.call("bot-a", "use", json!({"ids":[b["id"]]}))
            .await
            .unwrap()["affected"],
        0
    );
}

fn ratio(result: &Value, id: &Value) -> f64 {
    let hit = result["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|hit| &hit["memory"]["id"] == id)
        .unwrap();
    assert!((hit["relevance"].as_f64().unwrap() - 0.000001).abs() < 1e-12);
    hit["score"].as_f64().unwrap() / hit["relevance"].as_f64().unwrap()
}
fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-10, "{actual} != {expected}");
}

#[tokio::test]
async fn scoring_uses_importance_six_day_half_life_and_only_explicit_use() {
    let f = Fixture::new().await;
    let mut memories = Vec::new();
    for importance in ["low", "normal", "high"] {
        memories.push(f.call("bot", "create", json!({"kind":"fact","content":"body","keywords":["score"],"importance":importance})).await.unwrap()["memory"].clone());
    }
    let fresh = f.search(&["score"], None, None).await;
    close(ratio(&fresh, &memories[0]["id"]), 7.0 / 6.0);
    close(ratio(&fresh, &memories[1]["id"]), 1.5);
    close(ratio(&fresh, &memories[2]["id"]), 11.0 / 6.0);
    assert_eq!(fresh["results"][0]["memory"]["id"], memories[2]["id"]);
    f.days(6);
    let six = f.search(&["score"], None, None).await;
    close(ratio(&six, &memories[1]["id"]), 17.0 / 12.0);
    f.days(12);
    let twelve = f.search(&["score"], None, None).await;
    close(ratio(&twelve, &memories[1]["id"]), 11.0 / 8.0);
    let before = f.read(&memories[1]["id"]).await;
    assert!(before["last_read_at"].is_null());
    f.call("bot", "use", json!({"ids":[before["id"],before["id"]]}))
        .await
        .unwrap();
    let used = f.read(&before["id"]).await;
    assert_ne!(used["last_read_at"], before["last_read_at"]);
    let mut expected = before.clone();
    expected["last_read_at"] = used["last_read_at"].clone();
    assert_eq!(used, expected);
    close(
        ratio(&f.search(&["score"], None, None).await, &before["id"]),
        1.5,
    );
    f.days(18);
    close(
        ratio(&f.search(&["score"], None, None).await, &before["id"]),
        17.0 / 12.0,
    );
    f.days(-1);
    close(
        ratio(&f.search(&["score"], None, None).await, &before["id"]),
        1.5,
    );
}

#[tokio::test]
async fn filtering_precedes_candidates_and_limit_refers_to_reranked_candidates() {
    let f = Fixture::new().await;
    let mut memories = Vec::new();
    for _ in 0..105 {
        memories.push(f.create(Some("outside"), &["candidate"]).await);
    }
    let scoped = f.create(Some("inside"), &["candidate"]).await;
    assert_eq!(
        f.search(&["candidate"], Some("inside"), None).await["results"][0]["memory"]["id"],
        scoped["id"]
    );
    assert_eq!(
        f.search(&["candidate"], Some("inside"), None).await["has_more"],
        false
    );
    memories.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    let last = memories.last().unwrap();
    f.call("dreaming", "update", json!({"id":last["id"],"kind":"fact","content":"best outside the candidate set","keywords":["candidate"],"importance":"high"})).await.unwrap();
    let result = f.search(&["candidate"], Some("outside"), None).await;
    assert_eq!(result["results"].as_array().unwrap().len(), 10);
    assert_eq!(result["has_more"], true);
    assert!(
        result["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|hit| hit["memory"]["id"] != last["id"])
    );
    let within = &memories[99];
    f.call("dreaming", "update", json!({"id":within["id"],"kind":"fact","content":"best candidate","keywords":["candidate"],"importance":"high"})).await.unwrap();
    let result = f.search(&["candidate"], Some("outside"), Some(1)).await;
    assert_eq!(result["results"][0]["memory"]["id"], within["id"]);
    assert_eq!(
        f.search(&["candidate"], Some("outside"), Some(30)).await["results"]
            .as_array()
            .unwrap()
            .len(),
        30
    );
    for memory in &memories[..100] {
        f.archive(&memory["id"], true).await;
    }
    let result = f.search(&["candidate"], Some("outside"), None).await;
    assert_eq!(result["results"].as_array().unwrap().len(), 5);
    assert_eq!(result["results"][0]["memory"]["id"], last["id"]);
    assert_eq!(result["has_more"], false);
}

#[tokio::test]
async fn content_updates_and_feedback_preserve_each_other_in_both_orders_and_concurrently() {
    let f = Fixture::new().await;
    let original = f.create(None, &["before"]).await;
    let update = json!({"id":original["id"],"kind":"lesson","content":"updated","keywords":["after"],"importance":"high"});
    f.days(1);
    f.call("bot", "use", json!({"ids":[original["id"]]}))
        .await
        .unwrap();
    let first_use = f.read(&original["id"]).await["last_read_at"].clone();
    f.days(2);
    f.call("dreaming", "update", update.clone()).await.unwrap();
    let updated = f.read(&original["id"]).await;
    assert_eq!(updated["last_read_at"], first_use);
    f.days(3);
    f.call("bot", "use", json!({"ids":[original["id"]]}))
        .await
        .unwrap();
    let used = f.read(&original["id"]).await;
    assert_eq!(used["updated_at"], updated["updated_at"]);
    assert_eq!(used["content"], "updated");
    assert_ne!(used["last_read_at"], first_use);
    f.days(4);
    let mut update = update;
    update["content"] = json!("concurrent content");
    let (write, feedback) = tokio::join!(
        f.call("dreaming", "update", update),
        f.call("bot", "use", json!({"ids":[original["id"]]}))
    );
    write.unwrap();
    feedback.unwrap();
    let final_memory = f.read(&original["id"]).await;
    assert_eq!(final_memory["content"], "concurrent content");
    assert_eq!(final_memory["updated_at"], final_memory["last_read_at"]);
    assert_ne!(final_memory["last_read_at"], used["last_read_at"]);
    assert_eq!(
        f.search(&["after"], None, None).await["results"][0]["memory"]["id"],
        original["id"]
    );
    assert_eq!(
        f.search(&["before"], None, None).await["results"],
        json!([])
    );
}

#[tokio::test]
async fn unfinished_entries_survive_partial_work_and_completion_does_not_change_usage() {
    let f = Fixture::new().await;
    let a = f.create(Some("a"), &["new"]).await;
    let b = f.create(Some("b"), &["other"]).await;
    let archived = f.create(None, &["archived"]).await;
    f.archive(&archived["id"], true).await;
    assert_eq!(a["organized"], false);
    let first = f
        .call("dreaming", "pending", json!({"limit":1}))
        .await
        .unwrap();
    assert_eq!(first["memories"][0]["id"], a["id"]);
    f.call("bot", "use", json!({"ids":[a["id"]]}))
        .await
        .unwrap();
    let used = f.read(&a["id"]).await;
    f.call("dreaming", "update", json!({"id":a["id"],"kind":"fact","content":"partially updated","keywords":["new"],"importance":"normal","sources":a["sources"]})).await.unwrap();
    // A later invocation still sees the changed but unfinished item.
    let pending = f.call("dreaming", "pending", json!({})).await.unwrap();
    assert_eq!(pending["memories"].as_array().unwrap().len(), 2);
    assert_eq!(pending["memories"][0]["content"], "partially updated");
    f.call(
        "dreaming",
        "organize",
        json!({"id":a["id"],"organized":true,"reason":"Checked evidence and observations"}),
    )
    .await
    .unwrap();
    let rest = f
        .call("dreaming", "pending", json!({"after":first["next_cursor"]}))
        .await
        .unwrap();
    assert_eq!(rest["memories"][0]["id"], b["id"]);
    assert_eq!(rest["next_cursor"], Value::Null);
    let completed = f.read(&a["id"]).await;
    assert_eq!(completed["last_read_at"], used["last_read_at"]);
    assert_eq!(completed["created_at"], a["created_at"]);
    assert_eq!(completed["workspace_id"], a["workspace_id"]);
    assert_eq!(completed["sources"], a["sources"]);
    for caller in ["bot", "reader"] {
        assert!(matches!(
            f.call(caller, "pending", json!({})).await,
            Err(Error::Forbidden)
        ));
        assert!(matches!(
            f.call(
                caller,
                "organize",
                json!({"id":b["id"],"organized":true,"reason":"done"})
            )
            .await,
            Err(Error::Forbidden)
        ));
    }
    f.call("dreaming", "update", json!({"id":a["id"],"kind":"fact","content":"new evidence","keywords":["new"],"importance":"normal","sources":a["sources"]})).await.unwrap();
    assert_eq!(f.read(&a["id"]).await["organized"], false);
    f.archive(&archived["id"], false).await;
    assert_eq!(
        f.call("dreaming", "pending", json!({})).await.unwrap()["memories"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[tokio::test]
async fn dreaming_writes_require_an_explanation_before_mutating_memory() {
    let f = Fixture::new().await;
    let memory = f.create(None, &["reason"]).await;
    for reason in [Value::Null, json!("   ")] {
        let result = f
            .commands
            .execute(
                serde_json::from_value(json!({
                    "session_id":"dreaming", "reason":reason,
                    "command":{"action":"organize","input":{"id":memory["id"],"organized":true}}
                }))
                .unwrap(),
            )
            .await;
        assert!(matches!(result, Err(Error::InvalidInput(_))));
        assert_eq!(f.read(&memory["id"]).await["organized"], false);
    }
}
