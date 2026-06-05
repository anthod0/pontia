use super::*;

const ARTIFACT_PREVIEW_BYTES: usize = 1024;
const MAX_ARTIFACT_CONTENT_BYTES: i64 = 1024 * 1024;

#[derive(Clone)]
pub struct ArtifactDiscoveryService {
    pool: SqlitePool,
}

impl ArtifactDiscoveryService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn discover(&self, session_id: &str) -> Result<ArtifactDiscoveryOutcome> {
        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        let workspace = session.workspace.ok_or_else(|| {
            Error::Domain(format!(
                "session {session_id} does not have a workspace to discover"
            ))
        })?;
        let workspace_root = std::fs::canonicalize(&workspace).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                Error::NotFound(format!("session {session_id} workspace not found"))
            } else {
                Error::Io(err)
            }
        })?;
        if !workspace_root.is_dir() {
            return Err(Error::Domain(format!(
                "session {session_id} workspace is not a directory"
            )));
        }

        let mut discovered = Vec::new();
        collect_workspace_files(&workspace_root, &workspace_root, &mut discovered)?;
        discovered.sort();

        for path in discovered {
            let relative_path = path
                .strip_prefix(&workspace_root)
                .map_err(|_| Error::Domain("artifact path escaped workspace".to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            let metadata = std::fs::metadata(&path)?;
            let size_bytes = i64::try_from(metadata.len()).map_err(|_| {
                Error::Domain(format!("artifact {relative_path} is too large to index"))
            })?;
            let preview = preview_for_file(&path)?;
            let kind = infer_artifact_kind(&path, preview.as_deref());
            let modified_at = metadata
                .modified()
                .ok()
                .and_then(|time| time::OffsetDateTime::from(time).format(&Rfc3339).ok());
            let artifact_id = deterministic_artifact_id(session_id, &relative_path);
            let source_ref = format!("file://{}", path.display());
            let metadata = json!({
                "relative_path": relative_path,
                "modified_at": modified_at,
                "content_fingerprint": content_fingerprint(session_id, size_bytes, &source_ref),
                "preview": preview,
                "source_ref": source_ref,
            });

            sqlx::query(
                r#"INSERT INTO artifacts
                   (artifact_id, session_id, turn_id, kind, name, source_ref, size_bytes, metadata)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                   ON CONFLICT(artifact_id) DO UPDATE SET
                       session_id = excluded.session_id,
                       turn_id = excluded.turn_id,
                       kind = excluded.kind,
                       name = excluded.name,
                       source_ref = excluded.source_ref,
                       size_bytes = excluded.size_bytes,
                       metadata = excluded.metadata,
                       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
            )
            .bind(&artifact_id)
            .bind(session_id)
            .bind(Option::<String>::None)
            .bind(kind)
            .bind(&relative_path)
            .bind(&source_ref)
            .bind(size_bytes)
            .bind(serde_json::to_string(&metadata)?)
            .execute(&self.pool)
            .await?;
        }

        let artifacts = query.list_artifacts(session_id).await?;
        Ok(ArtifactDiscoveryOutcome { artifacts })
    }
}

#[derive(Clone)]
pub struct ArtifactRegistrationService {
    pool: SqlitePool,
}

impl ArtifactRegistrationService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn register(&self, artifact: ArtifactRegistration) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO artifacts
               (artifact_id, session_id, turn_id, kind, name, source_ref, size_bytes, metadata)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(artifact_id) DO UPDATE SET
                   session_id = excluded.session_id,
                   turn_id = excluded.turn_id,
                   kind = excluded.kind,
                   name = excluded.name,
                   source_ref = excluded.source_ref,
                   size_bytes = excluded.size_bytes,
                   metadata = excluded.metadata,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
        )
        .bind(&artifact.artifact_id)
        .bind(&artifact.session_id)
        .bind(&artifact.turn_id)
        .bind(&artifact.kind)
        .bind(&artifact.name)
        .bind(&artifact.source_ref)
        .bind(artifact.size_bytes)
        .bind(serde_json::to_string(&artifact.metadata)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct ArtifactContentService {
    pool: SqlitePool,
}

impl ArtifactContentService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn read_content(&self, artifact_id: &str) -> Result<ArtifactContent> {
        let row = sqlx::query("SELECT source_ref, size_bytes FROM artifacts WHERE artifact_id = ?")
            .bind(artifact_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| Error::NotFound(format!("artifact {artifact_id} not found")))?;

        let source_ref: String = row.try_get("source_ref")?;
        let expected_size: Option<i64> = row.try_get("size_bytes")?;
        if let Some(expected_size) = expected_size
            && expected_size > MAX_ARTIFACT_CONTENT_BYTES
        {
            return Err(Error::Domain(format!(
                "artifact {artifact_id} is too large to read through the content API"
            )));
        }
        let path = artifact_file_path(&source_ref)?;
        let bytes = std::fs::read(&path).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                Error::NotFound(format!(
                    "artifact {artifact_id} content not found at registered source"
                ))
            } else {
                Error::Io(err)
            }
        })?;

        if let Some(expected_size) = expected_size
            && expected_size >= 0
            && bytes.len() as i64 != expected_size
        {
            return Err(Error::StateConflict(format!(
                "artifact {artifact_id} metadata size {expected_size} does not match content size {}",
                bytes.len()
            )));
        }

        Ok(ArtifactContent { bytes })
    }
}

fn artifact_file_path(source_ref: &str) -> Result<PathBuf> {
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

fn collect_workspace_files(root: &Path, current: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        if file_name.to_string_lossy() == ".pilotfy" {
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

fn preview_for_file(path: &Path) -> Result<Option<String>> {
    let bytes = std::fs::read(path)?;
    if bytes.contains(&0) {
        return Ok(None);
    }
    let preview_bytes = bytes.len().min(ARTIFACT_PREVIEW_BYTES);
    let text = String::from_utf8_lossy(&bytes[..preview_bytes]).to_string();
    Ok(Some(text))
}

fn infer_artifact_kind(path: &Path, preview: Option<&str>) -> &'static str {
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

fn deterministic_artifact_id(session_id: &str, relative_path: &str) -> String {
    format!("art_{:016x}", stable_hash(&(session_id, relative_path)))
}

fn content_fingerprint(session_id: &str, size_bytes: i64, source_ref: &str) -> String {
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
