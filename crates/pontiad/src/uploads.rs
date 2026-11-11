use std::{path::PathBuf, time::Duration};

use pontia_application::uploads::UploadService;
use pontia_config::UploadsConfig;
use time::OffsetDateTime;
use tokio::{sync::watch, time::MissedTickBehavior};

pub async fn run(home: PathBuf, config: UploadsConfig, mut shutdown: watch::Receiver<bool>) {
    let service = UploadService::new(home);
    let mut interval = tokio::time::interval(Duration::from_secs(
        u64::from(config.cleanup_interval_hours.get()) * 60 * 60,
    ));
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            _ = shutdown.wait_for(|stop| *stop) => break,
            _ = interval.tick() => {}
        }
        let service = service.clone();
        match tokio::task::spawn_blocking(move || {
            service.cleanup(&config, OffsetDateTime::now_utc())
        })
        .await
        {
            Ok(Ok(deleted)) => tracing::info!(deleted, "upload cleanup completed"),
            Ok(Err(error)) => tracing::warn!(%error, "upload cleanup failed"),
            Err(error) => tracing::error!(%error, "upload cleanup task failed"),
        }
    }
}
