use std::{
    ffi::OsStr,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use tracing::warn;

use super::{ResolvedDashboard, archive};

pub(super) async fn resolve(source: &str, pontia_home: &Path) -> ResolvedDashboard {
    let cache_dir = pontia_home.join("cache/dashboard");
    if let Err(error) = validate_cache_boundary(pontia_home, &cache_dir) {
        return ResolvedDashboard::unavailable(error);
    }
    let current_dir = cache_dir.join("current");

    match refresh(pontia_home, source, &cache_dir, &current_dir).await {
        Ok(root) => ResolvedDashboard::available(root),
        Err(err) => {
            warn!(source, cache_dir = %cache_dir.display(), error = %err, "failed to refresh remote dashboard cache");
            if let Err(cache_err) = validate_cache_boundary(pontia_home, &cache_dir)
                .and_then(|()| reject_symlink(Some(&current_dir), "current dashboard cache"))
            {
                return ResolvedDashboard::unavailable(format!(
                    "failed to refresh remote dashboard ({err}); refusing cached dashboard: {cache_err}"
                ));
            }
            match cached_dashboard_root(&current_dir) {
                Ok(root) => ResolvedDashboard::available(root),
                Err(cache_err) => ResolvedDashboard::unavailable(format!(
                    "failed to refresh remote dashboard ({err}); no usable cached dashboard: {cache_err}"
                )),
            }
        }
    }
}

async fn refresh(
    pontia_home: &Path,
    source: &str,
    cache_dir: &Path,
    current_dir: &Path,
) -> Result<PathBuf, String> {
    validate_cache_boundary(pontia_home, cache_dir)?;
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
    tokio::fs::create_dir(&staging_dir)
        .await
        .map_err(|err| format!("failed to create unique staging dir: {err}"))?;

    let extract_result = archive::extract(source, &bytes, &staging_dir)
        .and_then(|()| find_unique_index_parent(&staging_dir));

    match extract_result {
        Ok(_) => {
            if tokio::fs::try_exists(current_dir).await.unwrap_or(false) {
                remove_cache_tree(pontia_home, cache_dir, current_dir, CacheTreeKind::Current)
                    .await
                    .map_err(|err| format!("failed to replace cached dashboard: {err}"))?;
            }
            tokio::fs::rename(&staging_dir, current_dir)
                .await
                .map_err(|err| format!("failed to publish cached dashboard: {err}"))?;
            cached_dashboard_root(current_dir)
        }
        Err(err) => {
            let _ = remove_cache_tree(pontia_home, cache_dir, &staging_dir, CacheTreeKind::Staging)
                .await;
            Err(err)
        }
    }
}

#[derive(Clone, Copy)]
enum CacheTreeKind {
    Current,
    Staging,
}

async fn remove_cache_tree(
    pontia_home: &Path,
    cache_dir: &Path,
    target: &Path,
    kind: CacheTreeKind,
) -> Result<(), String> {
    let name = target
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| "dashboard cache cleanup target has no valid name".to_string())?;
    let expected_name = match kind {
        CacheTreeKind::Current => name == "current",
        CacheTreeKind::Staging => name.starts_with("staging-") && name.len() > "staging-".len(),
    };
    if !expected_name || target.parent() != Some(cache_dir) || target == cache_dir {
        return Err(format!(
            "refusing unsafe dashboard cache cleanup target {}",
            target.display()
        ));
    }
    validate_cache_boundary(pontia_home, cache_dir)?;
    reject_symlink(Some(target), "dashboard cache cleanup target")?;
    tokio::fs::remove_dir_all(target)
        .await
        .map_err(|err| err.to_string())
}

fn validate_cache_boundary(pontia_home: &Path, cache_dir: &Path) -> Result<(), String> {
    if pontia_home.as_os_str().is_empty()
        || !pontia_home.is_absolute()
        || pontia_home.parent().is_none()
        || pontia_home
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        || cache_dir != pontia_home.join("cache/dashboard")
    {
        return Err(format!(
            "refusing unsafe dashboard cache root {}",
            cache_dir.display()
        ));
    }
    reject_symlink(Some(pontia_home), "Pontia home")?;
    reject_symlink(Some(&pontia_home.join("cache")), "dashboard cache parent")?;
    reject_symlink(Some(cache_dir), "dashboard cache")
}

fn reject_symlink(path: Option<&Path>, description: &str) -> Result<(), String> {
    let Some(path) = path else {
        return Err(format!("{description} has no parent"));
    };
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "refusing {description} through symbolic link {}",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to inspect {description}: {error}")),
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

fn unique_suffix() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("{}-{millis}", std::process::id())
}
