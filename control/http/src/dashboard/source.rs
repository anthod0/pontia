use std::{env, path::PathBuf};

use pontia_config::DashboardConfig;

use super::{ResolvedDashboard, remote_cache};

pub async fn resolve_dashboard(config: &DashboardConfig) -> ResolvedDashboard {
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
        remote_cache::resolve(source).await
    } else {
        resolve_local_dashboard(source).await
    }
}

async fn resolve_local_dashboard(source: &str) -> ResolvedDashboard {
    let root = expand_tilde(source);
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

fn expand_tilde(value: &str) -> PathBuf {
    if value == "~" {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(value));
    }
    if let Some(rest) = value.strip_prefix("~/")
        && let Some(home) = env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(value)
}

fn is_remote_source(source: &str) -> bool {
    source.starts_with("http://") || source.starts_with("https://")
}
