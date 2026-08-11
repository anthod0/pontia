use std::{
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use tracing::warn;

use super::{ResolvedDashboard, archive};

pub(super) async fn resolve(source: &str) -> ResolvedDashboard {
    let cache_dir = default_cache_dir();
    let current_dir = cache_dir.join("current");

    match refresh(source, &cache_dir, &current_dir).await {
        Ok(root) => ResolvedDashboard::available(root),
        Err(err) => {
            warn!(source, cache_dir = %cache_dir.display(), error = %err, "failed to refresh remote dashboard cache");
            match cached_dashboard_root(&current_dir) {
                Ok(root) => ResolvedDashboard::available(root),
                Err(cache_err) => ResolvedDashboard::unavailable(format!(
                    "failed to refresh remote dashboard ({err}); no usable cached dashboard: {cache_err}"
                )),
            }
        }
    }
}

async fn refresh(source: &str, cache_dir: &Path, current_dir: &Path) -> Result<PathBuf, String> {
    tokio::fs::create_dir_all(cache_dir)
        .await
        .map_err(|err| format!("failed to create cache dir: {err}"))?;

    let response = reqwest::get(source)
        .await
        .map_err(|err| format!("failed to download archive: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("download returned HTTP {}", response.status()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|err| format!("failed to read archive body: {err}"))?;

    let staging_dir = cache_dir.join(format!("staging-{}", unique_suffix()));
    tokio::fs::create_dir_all(&staging_dir)
        .await
        .map_err(|err| format!("failed to create staging dir: {err}"))?;

    let extract_result = archive::extract(source, &bytes, &staging_dir)
        .and_then(|()| find_unique_index_parent(&staging_dir));

    match extract_result {
        Ok(_) => {
            if tokio::fs::try_exists(current_dir).await.unwrap_or(false) {
                tokio::fs::remove_dir_all(current_dir)
                    .await
                    .map_err(|err| format!("failed to replace cached dashboard: {err}"))?;
            }
            tokio::fs::rename(&staging_dir, current_dir)
                .await
                .map_err(|err| format!("failed to publish cached dashboard: {err}"))?;
            cached_dashboard_root(current_dir)
        }
        Err(err) => {
            let _ = tokio::fs::remove_dir_all(&staging_dir).await;
            Err(err)
        }
    }
}

fn cached_dashboard_root(current_dir: &Path) -> Result<PathBuf, String> {
    find_unique_index_parent(current_dir)
}

fn find_unique_index_parent(root: &Path) -> Result<PathBuf, String> {
    let mut matches = Vec::new();
    collect_index_files(root, &mut matches)?;
    match matches.len() {
        1 => matches[0]
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| "index.html has no parent directory".to_string()),
        0 => Err("archive does not contain index.html".to_string()),
        count => Err(format!(
            "archive contains {count} index.html files; expected exactly one"
        )),
    }
}

fn collect_index_files(root: &Path, matches: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(root)
        .map_err(|err| format!("failed to read {}: {err}", root.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("failed to read directory entry: {err}"))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|err| format!("failed to read file type: {err}"))?;
        if file_type.is_dir() {
            collect_index_files(&path, matches)?;
        } else if path.file_name() == Some(OsStr::new("index.html")) {
            matches.push(path);
        }
    }
    Ok(())
}

fn default_cache_dir() -> PathBuf {
    match env::var_os("PONTIA_HOME") {
        Some(home) if !home.is_empty() => PathBuf::from(home).join("cache/dashboard"),
        _ => match env::var_os("HOME") {
            Some(home) => PathBuf::from(home).join(".pontia/cache/dashboard"),
            None => PathBuf::from(".pontia/cache/dashboard"),
        },
    }
}

fn unique_suffix() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("{}-{millis}", std::process::id())
}
