use std::path::{Path, PathBuf};

use pontia_config::DashboardConfig;

use super::{ResolvedDashboard, remote_cache};

pub async fn resolve_dashboard(config: &DashboardConfig, pontia_home: &Path) -> ResolvedDashboard {
    let Some(source) = config
        .source
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    else {
        return ResolvedDashboard::unavailable(
            "Dashboard source is not configured. Set [dashboard].source or PONTIA_DASHBOARD_SOURCE."
                .to_string(),
        );
    };

    if is_remote_source(source) {
        remote_cache::resolve(source, &pontia_home.join("cache/dashboard")).await
    } else {
        resolve_local_dashboard(source).await
    }
}

async fn resolve_local_dashboard(source: &str) -> ResolvedDashboard {
    let root = PathBuf::from(source);
    match tokio::fs::metadata(root.join("index.html")).await {
        Ok(metadata) if metadata.is_file() => ResolvedDashboard::available(root),
        Ok(_) => ResolvedDashboard::unavailable(format!(
            "dashboard entrypoint not found: {} is not a file",
            root.join("index.html").display()
        )),
        Err(err) => ResolvedDashboard::unavailable(format!(
            "dashboard entrypoint not found: {} ({err})",
            root.join("index.html").display()
        )),
    }
}

fn is_remote_source(source: &str) -> bool {
    source.starts_with("http://") || source.starts_with("https://")
}
