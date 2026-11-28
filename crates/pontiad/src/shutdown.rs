use std::io::{BufRead, Write};

use pontia_agent_control::runtime::shutdown::{Shutdown, ShutdownRequest, ShutdownUpdate};
use tokio_stream::StreamExt;

pub(crate) async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let line = std::io::stdin()
        .lock()
        .lines()
        .next()
        .transpose()?
        .unwrap_or_default();
    let request: ShutdownRequest = serde_json::from_str(&line)?;
    let vars = std::collections::HashMap::from([(
        "PONTIA_HOME".into(),
        request.pontia_home.display().to_string(),
    )]);
    let config = pontia_config::AppConfig::from_vars(&vars)?;
    let clients = pontia_clients::registration();
    let shutdown = Shutdown::inspect(
        &config.pontia_home,
        &clients,
        existing_database(&config.database_url),
    )
    .await?;
    let updates = shutdown.execute(request)?;
    tokio::pin!(updates);
    while let Some(update) = updates.next().await {
        if let ShutdownUpdate::Failed(message) = update {
            return Err(message.into());
        }
        let mut out = std::io::stdout().lock();
        serde_json::to_writer(&mut out, &update)?;
        writeln!(out)?;
        out.flush()?;
    }
    Ok(())
}

/// Open only an existing control-plane database. A first-run home has no records.
async fn existing_database(url: &str) -> pontia_core::Result<Option<sqlx::SqlitePool>> {
    let options: sqlx::sqlite::SqliteConnectOptions = url.parse()?;
    if !options.get_filename().try_exists()? {
        return Ok(None);
    }
    Ok(Some(sqlx::SqlitePool::connect_with(options).await?))
}
