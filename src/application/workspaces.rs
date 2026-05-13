use super::*;

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
pub struct WorkspaceBrowserConfig {
    pub roots: Vec<WorkspaceRootConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WorkspaceRootConfig {
    pub root_id: String,
    pub label: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkspaceRootView {
    pub root_id: String,
    pub label: String,
    pub canonical_path: Option<String>,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkspaceDirectoryEntryView {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub is_workspace: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkspaceDirectoryListingView {
    pub root_id: String,
    pub path: String,
    pub canonical_path: String,
    pub parent_path: Option<String>,
    pub entries: Vec<WorkspaceDirectoryEntryView>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RegisterWorkspaceRequest {
    pub root_id: String,
    #[serde(default)]
    pub path: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkspaceRecord {
    pub(crate) workspace_id: String,
    pub(crate) canonical_path: String,
}

pub(crate) async fn upsert_workspace(
    pool: &SqlitePool,
    workspace: &str,
) -> Result<WorkspaceRecord> {
    let input_path = PathBuf::from(workspace);
    std::fs::create_dir_all(&input_path)?;
    let canonical_path = std::fs::canonicalize(&input_path)?.display().to_string();
    upsert_canonical_workspace(pool, &canonical_path, None).await
}

pub(crate) async fn upsert_canonical_workspace(
    pool: &SqlitePool,
    canonical_path: &str,
    requested_name: Option<&str>,
) -> Result<WorkspaceRecord> {
    let display_path = canonical_path.to_string();
    let name = requested_name
        .filter(|name| !name.trim().is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            Path::new(&canonical_path)
                .file_name()
                .and_then(|name| name.to_str())
                .map(ToString::to_string)
        });

    if let Some(row) =
        sqlx::query("SELECT workspace_id, canonical_path FROM workspaces WHERE canonical_path = ?")
            .bind(canonical_path)
            .fetch_optional(pool)
            .await?
    {
        let workspace_id: String = row.try_get("workspace_id")?;
        sqlx::query(
            r#"UPDATE workspaces
               SET display_path = ?, name = COALESCE(?, name), state = 'active',
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   last_used_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE workspace_id = ?"#,
        )
        .bind(&display_path)
        .bind(&name)
        .bind(&workspace_id)
        .execute(pool)
        .await?;
        return Ok(WorkspaceRecord {
            workspace_id,
            canonical_path: canonical_path.to_string(),
        });
    }

    let workspace_id = new_workspace_id().to_string();
    sqlx::query(
        r#"INSERT INTO workspaces
           (workspace_id, canonical_path, display_path, name, last_used_at)
           VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))"#,
    )
    .bind(&workspace_id)
    .bind(canonical_path)
    .bind(&display_path)
    .bind(&name)
    .execute(pool)
    .await?;

    Ok(WorkspaceRecord {
        workspace_id,
        canonical_path: canonical_path.to_string(),
    })
}

pub(crate) async fn get_workspace_record(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<Option<WorkspaceRecord>> {
    sqlx::query("SELECT workspace_id, canonical_path FROM workspaces WHERE workspace_id = ?")
        .bind(workspace_id)
        .fetch_optional(pool)
        .await?
        .map(|row| {
            Ok(WorkspaceRecord {
                workspace_id: row.try_get("workspace_id")?,
                canonical_path: row.try_get("canonical_path")?,
            })
        })
        .transpose()
}

#[derive(Clone)]
pub struct WorkspaceBrowserService {
    pool: SqlitePool,
    config: WorkspaceBrowserConfig,
}

impl WorkspaceBrowserService {
    pub fn new(pool: SqlitePool, config: WorkspaceBrowserConfig) -> Self {
        Self { pool, config }
    }

    pub async fn list_roots(&self) -> Vec<WorkspaceRootView> {
        self.config
            .roots
            .iter()
            .map(|root| match std::fs::canonicalize(&root.path) {
                Ok(path) if path.is_dir() => WorkspaceRootView {
                    root_id: root.root_id.clone(),
                    label: root.label.clone(),
                    canonical_path: Some(path.display().to_string()),
                    state: "available".to_string(),
                },
                Ok(path) => WorkspaceRootView {
                    root_id: root.root_id.clone(),
                    label: root.label.clone(),
                    canonical_path: Some(path.display().to_string()),
                    state: "unavailable".to_string(),
                },
                Err(_) => WorkspaceRootView {
                    root_id: root.root_id.clone(),
                    label: root.label.clone(),
                    canonical_path: None,
                    state: "unavailable".to_string(),
                },
            })
            .collect()
    }

    pub async fn list_entries(
        &self,
        root_id: &str,
        relative_path: &str,
    ) -> Result<WorkspaceDirectoryListingView> {
        let root = self.root(root_id)?;
        let root_path = canonical_root(root)?;
        let requested_path = resolve_relative_path(&root_path, relative_path)?;
        if !requested_path.is_dir() {
            return Err(Error::NotFound(format!(
                "directory {relative_path:?} not found under workspace root {root_id}"
            )));
        }

        let mut entries = Vec::new();
        let mut warnings = Vec::new();
        let mut dir_entries = std::fs::read_dir(&requested_path)
            .map_err(Error::Io)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Error::Io)?;
        dir_entries.sort_by_key(|entry| entry.file_name());

        for entry in dir_entries {
            let name = entry.file_name().to_string_lossy().to_string();
            if should_skip_directory(&name) {
                continue;
            }
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(err) => {
                    warnings.push(format!("failed to inspect {name}: {err}"));
                    continue;
                }
            };
            if !file_type.is_dir() || file_type.is_symlink() {
                continue;
            }
            let canonical = match std::fs::canonicalize(entry.path()) {
                Ok(path) if path.starts_with(&root_path) => path,
                Ok(_) => continue,
                Err(err) => {
                    warnings.push(format!("failed to resolve {name}: {err}"));
                    continue;
                }
            };
            let entry_relative = canonical
                .strip_prefix(&root_path)
                .map_err(|_| Error::Domain("directory escaped workspace root".to_string()))?;
            let path = path_to_api_relative(entry_relative);
            let is_workspace = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM workspaces WHERE canonical_path = ?",
            )
            .bind(canonical.display().to_string())
            .fetch_one(&self.pool)
            .await?
                > 0;
            entries.push(WorkspaceDirectoryEntryView {
                name,
                path,
                kind: "directory".to_string(),
                is_workspace,
            });
        }

        let normalized_relative = if relative_path.trim().is_empty() {
            String::new()
        } else {
            path_to_api_relative(
                requested_path
                    .strip_prefix(&root_path)
                    .map_err(|_| Error::Domain("directory escaped workspace root".to_string()))?,
            )
        };
        let parent_path = Path::new(&normalized_relative)
            .parent()
            .map(path_to_api_relative)
            .filter(|path| path != &normalized_relative);

        Ok(WorkspaceDirectoryListingView {
            root_id: root_id.to_string(),
            path: normalized_relative,
            canonical_path: requested_path.display().to_string(),
            parent_path,
            entries,
            warnings,
        })
    }

    pub async fn register_workspace(
        &self,
        request: RegisterWorkspaceRequest,
    ) -> Result<WorkspaceView> {
        let root = self.root(&request.root_id)?;
        let root_path = canonical_root(root)?;
        let workspace_path = resolve_relative_path(&root_path, &request.path)?;
        if !workspace_path.exists() {
            return Err(Error::NotFound(format!(
                "workspace directory {:?} not found",
                request.path
            )));
        }
        if !workspace_path.is_dir() {
            return Err(Error::Domain(format!(
                "workspace path {:?} is not a directory",
                request.path
            )));
        }
        let record = upsert_canonical_workspace(
            &self.pool,
            &workspace_path.display().to_string(),
            request.name.as_deref(),
        )
        .await?;
        ExternalQueryService::new(self.pool.clone())
            .get_workspace(&record.workspace_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("workspace {} not found", record.workspace_id)))
    }

    fn root(&self, root_id: &str) -> Result<&WorkspaceRootConfig> {
        self.config
            .roots
            .iter()
            .find(|root| root.root_id == root_id)
            .ok_or_else(|| Error::NotFound(format!("workspace root {root_id} not found")))
    }
}

fn canonical_root(root: &WorkspaceRootConfig) -> Result<PathBuf> {
    let path = std::fs::canonicalize(&root.path)?;
    if !path.is_dir() {
        return Err(Error::NotFound(format!(
            "workspace root {} is not available",
            root.root_id
        )));
    }
    Ok(path)
}

fn resolve_relative_path(root: &Path, relative_path: &str) -> Result<PathBuf> {
    let relative = Path::new(relative_path.trim());
    if relative.is_absolute() {
        return Err(Error::Domain(
            "workspace browser path must be relative".to_string(),
        ));
    }
    for component in relative.components() {
        match component {
            std::path::Component::Normal(_) | std::path::Component::CurDir => {}
            _ => {
                return Err(Error::Domain(
                    "workspace browser path cannot escape the configured root".to_string(),
                ));
            }
        }
    }
    let candidate = if relative.as_os_str().is_empty() {
        root.to_path_buf()
    } else {
        root.join(relative)
    };
    let canonical = std::fs::canonicalize(&candidate).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            Error::NotFound(format!("directory {relative_path:?} not found"))
        } else {
            Error::Io(err)
        }
    })?;
    if !canonical.starts_with(root) {
        return Err(Error::Domain(
            "workspace browser path cannot escape the configured root".to_string(),
        ));
    }
    Ok(canonical)
}

fn path_to_api_relative(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn should_skip_directory(name: &str) -> bool {
    matches!(name, ".git" | "node_modules" | "target")
}
