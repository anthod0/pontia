mod cleanup;
mod codec;
mod store;
mod writer;
pub(crate) use store::{CacheKey, PageTurn};
pub use writer::TimelineCacheWriter;
pub(crate) use writer::{Backfill, page_turn, source_key};

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use pontia_core::{Error, Result};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteAutoVacuum, SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous},
};
use tokio::sync::{Mutex as AsyncMutex, OnceCell, OwnedMutexGuard};

const RETENTION: i64 = 14 * 86400;
const FORMAT_VERSION: i64 = 1;

#[derive(Clone)]
pub(crate) struct TimelineCache(Arc<Inner>);
type Flights = HashMap<(String, String), Weak<AsyncMutex<()>>>;

struct Inner {
    control: SqlitePool,
    directory: PathBuf,
    pool: OnceCell<SqlitePool>,
    flights: Mutex<Flights>,
}

impl TimelineCache {
    pub(crate) fn new(control: SqlitePool, home: &Path) -> Self {
        Self(Arc::new(Inner {
            control,
            directory: home.join("cache"),
            pool: OnceCell::new(),
            flights: Mutex::default(),
        }))
    }

    pub(crate) async fn flight(&self, session: &str, source: &str) -> OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self.0.flights.lock().unwrap();
            locks.retain(|_, lock| lock.strong_count() > 0);
            let slot = locks.entry((session.into(), source.into())).or_default();
            slot.upgrade().unwrap_or_else(|| {
                let lock = Arc::new(AsyncMutex::new(()));
                *slot = Arc::downgrade(&lock);
                lock
            })
        };
        lock.lock_owned().await
    }

    pub(crate) async fn revision(&self, session: &str) -> Result<(i64, i64, bool)> {
        Ok(sqlx::query_as("SELECT coalesce(max(CASE WHEN event_type='session.compaction_completed' THEN rowid END),0),coalesce(max(rowid),0), (SELECT state='exited' FROM sessions WHERE session_id=?1) FROM events WHERE session_id=?1 AND event_type IN ('session.resuming','session.ready','session.compaction_completed')")
            .bind(session).fetch_one(&self.0.control).await?)
    }

    pub(crate) async fn touch(&self, session: &str, at: i64) {
        if self.0.pool.get().is_none() && !self.0.directory.join("session-timeline.db").is_file() {
            return;
        }
        let update = async {
            sqlx::query("UPDATE cached_sessions SET last_active_at=max(last_active_at,?) WHERE session_id=? AND last_active_at<?")
                .bind(at).bind(session).bind(at).execute(self.db().await?).await?;
            Ok::<_, Error>(())
        };
        if let Err(error) = update.await {
            tracing::warn!(%error,session,"timeline cache activity update failed")
        }
    }

    pub(crate) async fn record_compaction(
        &self,
        event: &pontia_core::domain::DomainEvent,
    ) -> Result<()> {
        let db = self.db().await?;
        sqlx::query("INSERT OR IGNORE INTO cached_compactions SELECT session_id,?,? FROM cached_sessions WHERE session_id=?")
            .bind(&event.event_id).bind(event.occurred_at.unix_timestamp()).bind(&event.session_id).execute(db).await?;
        Ok(())
    }

    async fn db(&self) -> Result<&SqlitePool> {
        self.0
            .pool
            .get_or_try_init(|| open(&self.0.directory))
            .await
    }
}

async fn open(dir: &Path) -> Result<SqlitePool> {
    use std::{
        fs,
        os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    };
    let home = dir
        .parent()
        .ok_or_else(|| Error::Domain("missing timeline cache parent".into()))?;
    if !home.is_absolute()
        || home.parent().is_none()
        || home
            .components()
            .any(|c| c == std::path::Component::ParentDir)
        || !fs::symlink_metadata(home)?.is_dir()
    {
        return Err(Error::Domain(
            "invalid Pontia home for timeline cache".into(),
        ));
    }
    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700);
    match builder.create(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        result => result?,
    }
    if !fs::symlink_metadata(dir)?.is_dir() {
        return Err(Error::Domain("invalid timeline cache directory".into()));
    }
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    let path = dir.join("session-timeline.db");
    for name in [
        "session-timeline.db",
        "session-timeline.db-shm",
        "session-timeline.db-wal",
        "session-timeline.db-journal",
    ] {
        match fs::symlink_metadata(dir.join(name)) {
            Ok(m) if !m.is_file() => {
                return Err(Error::Domain("invalid timeline cache file".into()));
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => (),
        }
    }
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&path)?
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    match connect(&path).await {
        Err(Error::Database(sqlx::Error::Database(error)))
            if error
                .code()
                .as_deref()
                .and_then(|code| code.parse::<i32>().ok())
                .is_some_and(|code| matches!(code & 0xff, 11 | 26)) =>
        {
            tracing::warn!(%error,"rebuilding corrupt timeline cache");
            for suffix in ["-shm", "-wal", "-journal"] {
                let sidecar = dir.join(format!("session-timeline.db{suffix}"));
                if sidecar.try_exists()? {
                    fs::remove_file(sidecar)?;
                }
            }
            fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&path)?;
            connect(&path).await
        }
        result => result,
    }
}

async fn connect(path: &Path) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .auto_vacuum(SqliteAutoVacuum::Incremental)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5));
    let db = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_with(options)
        .await?;
    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&mut *tx)
        .await?;
    if version != 1 {
        sqlx::query("DROP TABLE IF EXISTS cached_checkpoints; DROP TABLE IF EXISTS cached_turns; DROP TABLE IF EXISTS cached_sources; DROP TABLE IF EXISTS cached_compactions; DROP TABLE IF EXISTS cached_sessions;").execute(&mut *tx).await?;
    }
    sqlx::query(include_str!("timeline_cache/schema.sql"))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(db)
}

fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

#[cfg(test)]
mod tests;
