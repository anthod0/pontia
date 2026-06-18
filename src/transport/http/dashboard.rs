use std::{
    env,
    ffi::OsStr,
    io::Cursor,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::State,
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use flate2::read::GzDecoder;
use tracing::warn;

use crate::config::DashboardConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDashboard {
    root: Option<PathBuf>,
    unavailable_reason: Option<String>,
}

impl ResolvedDashboard {
    pub fn local_default() -> Self {
        Self::available(default_dist_path())
    }

    pub fn available(root: PathBuf) -> Self {
        Self {
            root: Some(root),
            unavailable_reason: None,
        }
    }

    pub fn unavailable(reason: String) -> Self {
        Self {
            root: None,
            unavailable_reason: Some(reason),
        }
    }

    fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    fn unavailable_message(&self) -> String {
        self.unavailable_reason.clone().unwrap_or_else(|| {
            "Dashboard frontend has not been built. Run `pnpm --dir=apps/dashboard run build`."
                .to_string()
        })
    }
}

fn default_dist_path() -> PathBuf {
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("apps/dashboard/dist")
}

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
        resolve_remote_dashboard(source, config).await
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

async fn resolve_remote_dashboard(source: &str, config: &DashboardConfig) -> ResolvedDashboard {
    let cache_dir = config
        .cache_dir
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(expand_tilde)
        .unwrap_or_else(default_cache_dir);
    let current_dir = cache_dir.join("current");

    match refresh_remote_cache(source, &cache_dir, &current_dir).await {
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

async fn refresh_remote_cache(
    source: &str,
    cache_dir: &Path,
    current_dir: &Path,
) -> std::result::Result<PathBuf, String> {
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

    let extract_result = extract_archive(source, &bytes, &staging_dir)
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

fn cached_dashboard_root(current_dir: &Path) -> std::result::Result<PathBuf, String> {
    find_unique_index_parent(current_dir)
}

fn extract_archive(
    source: &str,
    bytes: &[u8],
    destination: &Path,
) -> std::result::Result<(), String> {
    let source_path = source.split(['?', '#']).next().unwrap_or(source);
    if source_path.ends_with(".zip") {
        extract_zip(bytes, destination)
    } else if source_path.ends_with(".tar.gz") || source_path.ends_with(".tgz") {
        extract_targz(bytes, destination)
    } else {
        Err("remote dashboard source must end with .zip, .tar.gz, or .tgz".to_string())
    }
}

fn extract_zip(bytes: &[u8], destination: &Path) -> std::result::Result<(), String> {
    let reader = Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|err| format!("invalid zip archive: {err}"))?;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|err| format!("failed to read zip entry: {err}"))?;
        let enclosed = file
            .enclosed_name()
            .ok_or_else(|| format!("unsafe zip entry path: {}", file.name()))?;
        let output = destination.join(enclosed);
        if file.is_dir() {
            std::fs::create_dir_all(&output)
                .map_err(|err| format!("failed to create zip directory: {err}"))?;
        } else {
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| format!("failed to create zip parent: {err}"))?;
            }
            let mut output_file = std::fs::File::create(&output)
                .map_err(|err| format!("failed to create zip output: {err}"))?;
            std::io::copy(&mut file, &mut output_file)
                .map_err(|err| format!("failed to write zip output: {err}"))?;
        }
    }
    Ok(())
}

fn extract_targz(bytes: &[u8], destination: &Path) -> std::result::Result<(), String> {
    let decoder = GzDecoder::new(Cursor::new(bytes));
    let mut archive = tar::Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|err| format!("invalid tar.gz archive: {err}"))?;
    for entry in entries {
        let mut entry = entry.map_err(|err| format!("failed to read tar entry: {err}"))?;
        let path = entry
            .path()
            .map_err(|err| format!("failed to read tar path: {err}"))?;
        let entry_type = entry.header().entry_type();
        if !(entry_type.is_file() || entry_type.is_dir()) {
            return Err(format!("unsupported tar entry type for {}", path.display()));
        }
        let safe_path = safe_relative_path(&path)?;
        entry
            .unpack(destination.join(safe_path))
            .map_err(|err| format!("failed to unpack tar entry: {err}"))?;
    }
    Ok(())
}

fn safe_relative_path(path: &Path) -> std::result::Result<PathBuf, String> {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            _ => return Err(format!("unsafe archive entry path: {}", path.display())),
        }
    }
    if clean.as_os_str().is_empty() {
        return Err("empty archive entry path".to_string());
    }
    Ok(clean)
}

fn find_unique_index_parent(root: &Path) -> std::result::Result<PathBuf, String> {
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

fn collect_index_files(root: &Path, matches: &mut Vec<PathBuf>) -> std::result::Result<(), String> {
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
    match env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(".cache/pontia/dashboard"),
        None => PathBuf::from(".cache/pontia/dashboard"),
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

fn unique_suffix() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("{}-{millis}", std::process::id())
}

pub async fn dashboard(State(dashboard): State<ResolvedDashboard>) -> Response {
    let Some(root) = dashboard.root() else {
        return (StatusCode::NOT_FOUND, dashboard.unavailable_message()).into_response();
    };

    match tokio::fs::read(root.join("index.html")).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], bytes).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, dashboard.unavailable_message()).into_response(),
    }
}

pub async fn dashboard_asset(State(dashboard): State<ResolvedDashboard>, uri: Uri) -> Response {
    let Some(relative_path) = uri.path().strip_prefix("/dashboard/") else {
        return StatusCode::NOT_FOUND.into_response();
    };
    dashboard_dist_file(dashboard, relative_path).await
}

pub async fn dashboard_path(State(dashboard): State<ResolvedDashboard>, uri: Uri) -> Response {
    let Some(relative_path) = uri.path().strip_prefix("/dashboard/") else {
        return StatusCode::NOT_FOUND.into_response();
    };

    match try_dashboard_dist_file(&dashboard, relative_path).await {
        Some(response) => response,
        None => self::dashboard(State(dashboard)).await,
    }
}

async fn dashboard_dist_file(dashboard: ResolvedDashboard, relative_path: &str) -> Response {
    match try_dashboard_dist_file(&dashboard, relative_path).await {
        Some(response) => response,
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn try_dashboard_dist_file(
    dashboard: &ResolvedDashboard,
    relative_path: &str,
) -> Option<Response> {
    let root = dashboard.root()?;
    let safe_path = safe_asset_path(relative_path)?;
    let path = root.join(&safe_path);
    let metadata = tokio::fs::metadata(&path).await.ok()?;
    if !metadata.is_file() {
        return None;
    }

    tokio::fs::read(path)
        .await
        .ok()
        .map(|bytes| ([(header::CONTENT_TYPE, content_type(relative_path))], bytes).into_response())
}

fn safe_asset_path(relative_path: &str) -> Option<PathBuf> {
    let path = Path::new(relative_path);
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            _ => return None,
        }
    }
    (!clean.as_os_str().is_empty()).then_some(clean)
}

fn content_type(path: &str) -> &'static str {
    if path.ends_with(".js") {
        "text/javascript; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".ico") {
        "image/x-icon"
    } else {
        "application/octet-stream"
    }
}
