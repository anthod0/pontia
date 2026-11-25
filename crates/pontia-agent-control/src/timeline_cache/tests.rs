use super::*;
use crate::{
    AgentBinding,
    client_contract::raw_transcripts::{TimelineItem, TurnTimelineItem},
};

struct Fixture {
    _root: tempfile::TempDir,
    cache: TimelineCache,
}
impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE sessions(session_id TEXT PRIMARY KEY, pinned_at TEXT, state TEXT, updated_at TEXT)").execute(&db).await.unwrap();
        let cache = TimelineCache::new(db, root.path());
        Self { _root: root, cache }
    }

    async fn session(&self, id: &str, pinned: bool, state: &str) -> AgentBinding {
        sqlx::query("INSERT INTO sessions VALUES(?, ?, ?, datetime(0,'unixepoch'))")
            .bind(id)
            .bind(pinned.then_some("pinned"))
            .bind(state)
            .execute(&self.cache.0.control)
            .await
            .unwrap();
        AgentBinding {
            id: format!("binding-{id}"),
            session_id: id.into(),
            client_type: "test".into(),
            launch_cwd: String::new(),
            client_session_key: id.into(),
            client_session_file: None,
            metadata: serde_json::Value::Null,
            discovered: true,
        }
    }
}

fn key(binding: &AgentBinding) -> CacheKey<'_> {
    CacheKey {
        binding,
        source: "default",
        version: 1,
        validator: None,
        generation: 0,
        revision: 0,
    }
}
fn turn() -> PageTurn<'static> {
    PageTurn {
        id: "turn",
        head: Some("head"),
        tail: Some("tail"),
        previous_tail: None,
        next_turn: None,
    }
}
fn item(role: &str, text: &str) -> TurnTimelineItem {
    TurnTimelineItem {
        turn_id: (role != "system").then(|| "turn".to_owned()),
        item: TimelineItem {
            item_id: role.into(),
            role: role.into(),
            kind: if role == "system" {
                "context_compaction"
            } else {
                role
            }
            .into(),
            raw_kind: None,
            title: None,
            status: None,
            occurred_at: Some("2026-01-02T12:34:56Z".into()),
            content_preview: text.to_owned(),
            call_id: None,
            managed_tool_use: None,
        },
    }
}

#[tokio::test]
async fn persisted_page_preserves_entries_positions_and_cursor_boundaries() {
    let f = Fixture::new().await;
    let binding = f.session("session", false, "exited").await;
    let key = key(&binding);
    let items = vec![
        item("user", "question"),
        item("system", "compacted"),
        item("assistant", &"response".repeat(1000)),
    ];
    f.cache.write(&key, &turn(), &items).await.unwrap();
    let disk = TimelineCache::new(f.cache.0.control.clone(), f._root.path());
    assert_eq!(disk.read(&key, &[turn()]).await.unwrap(), Some(items));
    let db = disk.db().await.unwrap();
    let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(db)
        .await
        .unwrap();
    let vacuum: i64 = sqlx::query_scalar("PRAGMA auto_vacuum")
        .fetch_one(db)
        .await
        .unwrap();
    assert_eq!(mode, "wal");
    assert_eq!(vacuum, 2);
    let bytes: Vec<u8> = sqlx::query_scalar("SELECT payload FROM cached_turns")
        .fetch_one(db)
        .await
        .unwrap();
    assert!(bytes.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]));
    assert_eq!(
        disk.read(
            &key,
            &[PageTurn {
                tail: Some("changed"),
                ..turn()
            }]
        )
        .await
        .unwrap(),
        None
    );
    assert_eq!(
        disk.read(
            &CacheKey {
                version: 2,
                ..self::key(&binding)
            },
            &[turn()]
        )
        .await
        .unwrap(),
        None
    );
    disk.write(&key, &turn(), &[]).await.unwrap();
    let raw: Vec<u8> = sqlx::query_scalar("SELECT payload FROM cached_turns")
        .fetch_one(db)
        .await
        .unwrap();
    assert_eq!(raw, b"[]");
}

#[tokio::test]
async fn cleanup_protects_pinned_running_viewed_and_recently_updated_sessions() {
    let f = Fixture::new().await;
    let at = now();
    for (id, pinned, state) in [
        ("expired", false, "exited"),
        ("pinned", true, "exited"),
        ("running", false, "idle"),
        ("viewed", false, "exited"),
        ("updated", false, "exited"),
        ("lru", false, "exited"),
    ] {
        let binding = f.session(id, pinned, state).await;
        f.cache
            .write(&key(&binding), &turn(), &[item("user", "old")])
            .await
            .unwrap();
        let age = if matches!(id, "lru" | "viewed") {
            86400
        } else {
            RETENTION + 5
        };
        sqlx::query("UPDATE cached_sessions SET last_active_at=? WHERE session_id=?")
            .bind(at - age)
            .bind(id)
            .execute(f.cache.db().await.unwrap())
            .await
            .unwrap();
        if id == "viewed" || id == "pinned" {
            assert!(
                f.cache
                    .read(&key(&binding), &[turn()])
                    .await
                    .unwrap()
                    .is_some()
            );
        }
        if id == "updated" {
            sqlx::query("UPDATE sessions SET updated_at=datetime('now') WHERE session_id=?")
                .bind(id)
                .execute(&f.cache.0.control)
                .await
                .unwrap();
        }
    }
    let db = f.cache.db().await.unwrap();
    sqlx::query("UPDATE cached_sessions SET last_active_at=0 WHERE session_id='pinned'")
        .execute(db)
        .await
        .unwrap();
    f.cache.cleanup(at, 2 * 1024 * 1024 * 1024).await.unwrap();
    let saved = || {
        sqlx::query_scalar::<_, String>(
            "SELECT session_id FROM cached_sessions ORDER BY session_id",
        )
        .fetch_all(db)
    };
    assert_eq!(
        saved().await.unwrap(),
        ["lru", "pinned", "running", "updated", "viewed"]
    );
    f.cache.cleanup(at, 1).await.unwrap();
    assert_eq!(
        saved().await.unwrap(),
        ["pinned", "running", "updated", "viewed"]
    );
}

#[tokio::test]
async fn expired_read_removes_cache_and_schema_change_rebuilds_only_cache() {
    let f = Fixture::new().await;
    let binding = f.session("expired", false, "exited").await;
    let key = key(&binding);
    f.cache
        .write(&key, &turn(), &[item("user", "first")])
        .await
        .unwrap();
    let db = f.cache.db().await.unwrap();
    sqlx::query("UPDATE cached_sessions SET last_active_at=0")
        .execute(db)
        .await
        .unwrap();
    let new_source = CacheKey {
        source: "native",
        ..self::key(&binding)
    };
    assert_eq!(f.cache.read(&new_source, &[turn()]).await.unwrap(), None);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM cached_turns")
        .fetch_one(db)
        .await
        .unwrap();
    assert_eq!(count, 0);
    f.cache.write(&key, &turn(), &[]).await.unwrap();
    sqlx::query("PRAGMA user_version=999")
        .execute(db)
        .await
        .unwrap();
    let reopened = TimelineCache::new(f.cache.0.control.clone(), f._root.path());
    assert_eq!(reopened.read(&key, &[turn()]).await.unwrap(), None);
    assert_eq!(reopened.activity("expired").await.unwrap(), (false, 0));
}

#[tokio::test]
async fn pre_compaction_rendering_survives_new_generation() {
    let f = Fixture::new().await;
    let binding = f.session("session", false, "idle").await;
    let old = CacheKey {
        validator: Some("original native file".into()),
        ..key(&binding)
    };
    let new = CacheKey {
        validator: Some("rewritten native file".into()),
        generation: 1,
        revision: 1,
        ..self::key(&binding)
    };
    let before = vec![item("assistant", "before compaction")];
    let after = vec![
        item("system", "compacted"),
        item("assistant", "after compaction"),
    ];
    f.cache.write(&old, &turn(), &before).await.unwrap();
    f.cache.write(&new, &turn(), &after).await.unwrap();
    assert_eq!(f.cache.read(&old, &[turn()]).await.unwrap(), Some(before));
    assert_eq!(f.cache.read(&new, &[turn()]).await.unwrap(), Some(after));
}

#[tokio::test]
async fn compaction_before_the_first_history_read_keeps_its_timestamp() {
    let f = Fixture::new().await;
    f.session("session", false, "idle").await;
    let mark = pontia_core::domain::DomainEvent::new(
        "compact".into(),
        "session".into(),
        None,
        pontia_core::domain::EventSource::AgentClient,
        "test".into(),
        pontia_core::domain::EventType::SessionCompactionCompleted,
        serde_json::json!({}),
    );
    f.cache.record_compaction(&mark).await.unwrap();
    let recorded:i64=sqlx::query_scalar("SELECT occurred_at FROM cached_compactions WHERE session_id='session' AND event_id='compact'")
        .fetch_one(f.cache.db().await.unwrap()).await.unwrap();
    assert_eq!(recorded, mark.occurred_at.unix_timestamp());
}

#[tokio::test]
async fn opening_cache_rejects_symlink_to_native_file() {
    let f = Fixture::new().await;
    std::fs::create_dir(&f.cache.0.directory).unwrap();
    let native = f._root.path().join("native");
    std::fs::write(&native, "native data").unwrap();
    std::os::unix::fs::symlink(&native, f.cache.0.directory.join("session-timeline.db")).unwrap();
    assert!(f.cache.db().await.is_err());
    assert_eq!(std::fs::read_to_string(native).unwrap(), "native data");
}

#[tokio::test]
async fn corrupt_database_is_rebuilt_without_changing_the_control_database() {
    let f = Fixture::new().await;
    let binding = f.session("session", false, "exited").await;
    std::fs::create_dir(&f.cache.0.directory).unwrap();
    std::fs::write(
        f.cache.0.directory.join("session-timeline.db"),
        b"broken sqlite database",
    )
    .unwrap();
    let key = key(&binding);
    let expected = vec![item("user", "restored")];
    f.cache.write(&key, &turn(), &expected).await.unwrap();
    assert_eq!(f.cache.read(&key, &[turn()]).await.unwrap(), Some(expected));
}
