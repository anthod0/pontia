pub mod models;
pub mod repositories;

use std::time::Duration;

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

    let options = database_url
        .parse::<SqliteConnectOptions>()?
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(10));

    prepare_database_file(&options)?;
    Ok(SqlitePool::connect_with(options.create_if_missing(true)).await?)
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

fn prepare_database_file(options: &SqliteConnectOptions) -> Result<()> {
    use sqlx::ConnectOptions;
    let path = options.get_filename();
    if path.as_os_str().is_empty() || path.as_os_str().as_encoded_bytes().starts_with(b"file:") {
        return Ok(());
    }
    // SQLx exposes the open mode through the URL. A relative filename is not necessarily a valid URL host.
    let url = options.clone().filename("/db").to_url_lossy();
    let mode = url
        .query_pairs()
        .find(|(key, _)| key == "mode")
        .map(|(_, value)| value);
    if mode.as_deref() == Some("memory") || (mode.as_deref() == Some("ro") && !path.exists()) {
        return Ok(());
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    {
        use std::{
            fs,
            io::ErrorKind,
            os::unix::fs::{OpenOptionsExt, PermissionsExt},
        };
        fn restrict_existing(path: &std::path::Path) -> std::io::Result<()> {
            if !fs::metadata(path)?.is_file() {
                return Err(std::io::Error::new(
                    ErrorKind::InvalidInput,
                    "expected a SQLite file",
                ));
            }
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        }

        // Closing an extra descriptor to an open SQLite database can release its POSIX locks.
        // Create exclusively or change an existing file's mode without opening it.
        match fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(path)
        {
            Ok(file) => file.set_permissions(fs::Permissions::from_mode(0o600))?,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => restrict_existing(path)?,
            Err(error) => return Err(error.into()),
        }
        for suffix in ["-wal", "-shm"] {
            let mut sidecar = path.as_os_str().to_owned();
            sidecar.push(suffix);
            if let Err(error) = restrict_existing(std::path::Path::new(&sidecar))
                && error.kind() != ErrorKind::NotFound
            {
                return Err(error.into());
            }
        }
    }
    Ok(())
}
