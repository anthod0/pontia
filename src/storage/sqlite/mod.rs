pub mod models;
pub mod repositories;

use std::path::Path;

use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

use crate::error::Result;

pub async fn connect_sqlite(database_url: &str) -> Result<SqlitePool> {
    let database_url = if sqlite_url_uses_home(database_url) {
        normalize_sqlite_database_url(database_url, &home_dir()?)?
    } else {
        database_url.to_string()
    };
    ensure_parent_dir(&database_url)?;

    let options = database_url
        .parse::<SqliteConnectOptions>()?
        .create_if_missing(true)
        .foreign_keys(true);

    Ok(SqlitePool::connect_with(options).await?)
}

pub async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

pub fn normalize_sqlite_database_url(database_url: &str, home: &str) -> Result<String> {
    let Some(path) = database_url.strip_prefix("sqlite://") else {
        return Ok(database_url.to_string());
    };

    if path == "~" {
        return Ok(format!("sqlite://{home}"));
    }

    if let Some(rest) = path.strip_prefix("~/") {
        return Ok(format!("sqlite://{home}/{rest}"));
    }

    Ok(database_url.to_string())
}

fn sqlite_url_uses_home(database_url: &str) -> bool {
    database_url
        .strip_prefix("sqlite://")
        .is_some_and(|path| path == "~" || path.starts_with("~/"))
}

fn home_dir() -> Result<String> {
    std::env::var("HOME").map_err(|_| crate::error::Error::InvalidConfig {
        key: "HOME",
        message: "required to expand sqlite://~ database URL".to_string(),
    })
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
