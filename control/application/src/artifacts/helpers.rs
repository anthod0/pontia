use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

use pontia_core::error::{Error, Result};

pub(super) const ARTIFACT_PREVIEW_BYTES: usize = 1024;
pub(super) const MAX_ARTIFACT_CONTENT_BYTES: i64 = 1024 * 1024;

pub(super) fn artifact_file_path(source_ref: &str) -> Result<PathBuf> {
    let Some(path) = source_ref.strip_prefix("file://") else {
        return Err(Error::Domain(
            "artifact content source is not a registered readable file source".to_string(),
        ));
    };

    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return Err(Error::Domain(
            "artifact file source must use an absolute path".to_string(),
        ));
    }

    Ok(path)
}

pub(super) fn collect_workspace_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        if file_name.to_string_lossy() == ".pontia" {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            let Ok(target) = std::fs::canonicalize(&path) else {
                continue;
            };
            if !target.starts_with(root) {
                continue;
            }
            if target.is_file() {
                files.push(target);
            } else if target.is_dir() {
                collect_workspace_files(root, &target, files)?;
            }
            continue;
        }
        if file_type.is_dir() {
            collect_workspace_files(root, &path, files)?;
        } else if file_type.is_file() {
            let canonical = std::fs::canonicalize(&path)?;
            if canonical.starts_with(root) {
                files.push(canonical);
            }
        }
    }
    Ok(())
}

pub(super) fn preview_for_file(path: &Path) -> Result<Option<String>> {
    let bytes = std::fs::read(path)?;
    if bytes.contains(&0) {
        return Ok(None);
    }
    let preview_bytes = bytes.len().min(ARTIFACT_PREVIEW_BYTES);
    let text = String::from_utf8_lossy(&bytes[..preview_bytes]).to_string();
    Ok(Some(text))
}

pub(super) fn infer_artifact_kind(path: &Path, preview: Option<&str>) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("md" | "markdown") => "markdown",
        Some("log") => "log",
        Some("patch" | "diff") => "patch",
        Some("json") => "json",
        Some("txt" | "text") => "text",
        Some("html" | "htm") => "html",
        _ if preview.is_some() => "text",
        _ => "binary",
    }
}

pub(super) fn deterministic_artifact_id(session_id: &str, relative_path: &str) -> String {
    format!("art_{:016x}", stable_hash(&(session_id, relative_path)))
}

pub(super) fn content_fingerprint(session_id: &str, size_bytes: i64, source_ref: &str) -> String {
    format!(
        "{:016x}",
        stable_hash(&(session_id, size_bytes, source_ref))
    )
}

fn stable_hash<T: Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}
