pub mod models;
pub mod repositories;

use std::{path::Path, time::Duration};

use pontia_core::{Error, Result};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
};

pub async fn connect_sqlite(database_url: &str) -> Result<SqlitePool> {
    if sqlite_url_uses_tilde(database_url) {
        return Err(Error::InvalidConfig {
            key: "database_url",
            message: "tilde-prefixed SQLite paths are not supported".to_string(),
        });
    }
    ensure_parent_dir(database_url)?;

    let options = database_url
        .parse::<SqliteConnectOptions>()?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(10));

    Ok(SqlitePool::connect_with(options).await?)
}

pub async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

fn sqlite_url_uses_tilde(database_url: &str) -> bool {
    database_url
        .strip_prefix("sqlite://")
        .is_some_and(|path| path == "~" || path.starts_with("~/"))
}

fn ensure_parent_dir(database_url: &str) -> Result<()> {
    let Some(path) = database_url.strip_prefix("sqlite://") else {
        return Ok(());
    };

    if path == ":memory:" || path.starts_with("file:") {
        return Ok(());
    }

    if let Some(parent) = Path::new(path)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }

    Ok(())
}
