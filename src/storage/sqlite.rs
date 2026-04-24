use std::path::Path;

use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

use crate::error::Result;

pub async fn connect_sqlite(database_url: &str) -> Result<SqlitePool> {
    ensure_parent_dir(database_url)?;

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
