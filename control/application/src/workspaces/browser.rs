use std::path::{Path, PathBuf};

use nucleo_matcher::{
    Config, Matcher,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};
use sqlx::SqlitePool;

use pontia_config::{FilePickerConfig, WorkspaceBrowserConfig, WorkspaceRootConfig};
use pontia_core::error::{Error, Result};
use pontia_storage_sqlite::repositories::workspaces::SqliteWorkspaceRepository;

use super::{
    FilePickerFileView, FilePickerResultView, RegisterWorkspaceRequest, RenameWorkspaceRequest,
    WorkspaceDirectoryEntryView, WorkspaceDirectoryListingView, WorkspaceRootView,
    helpers::{canonical_root, path_to_api_relative, resolve_relative_path, should_skip_directory},
    persistence::{get_workspace_record, upsert_canonical_workspace},
};
use crate::{ExternalQueryService, WorkspaceView};

#[derive(Clone)]
pub struct WorkspaceBrowserService {
    pool: SqlitePool,
    config: WorkspaceBrowserConfig,
    file_picker: FilePickerConfig,
}

impl WorkspaceBrowserService {
    pub fn new(pool: SqlitePool, config: WorkspaceBrowserConfig) -> Self {
        Self {
            pool,
            config,
            file_picker: FilePickerConfig::default(),
        }
    }

    pub fn with_file_picker(
        pool: SqlitePool,
        config: WorkspaceBrowserConfig,
        file_picker: FilePickerConfig,
    ) -> Self {
        Self {
            pool,
            config,
            file_picker,
        }
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
            let is_workspace = SqliteWorkspaceRepository::new(self.pool.clone())
                .active_workspace_exists_at_path(&canonical.display().to_string())
                .await?;
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

    pub async fn pick_files(
        &self,
        workspace_id: &str,
        query: &str,
        limit: Option<usize>,
    ) -> Result<FilePickerResultView> {
        let config = self.config_file_picker();
        if !config.enabled || query.chars().count() < config.min_query_chars {
            return Ok(FilePickerResultView {
                files: Vec::new(),
                truncated: false,
                warnings: Vec::new(),
            });
        }
        let workspace = get_workspace_record(&self.pool, workspace_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("workspace {workspace_id} not found")))?;
        let workspace_path = PathBuf::from(&workspace.canonical_path);
        let root_path = std::fs::canonicalize(&workspace_path)?;
        if !root_path.is_dir() {
            return Err(Error::NotFound(format!(
                "workspace {workspace_id} directory is not available"
            )));
        }

        let mut override_builder = ignore::overrides::OverrideBuilder::new(&root_path);
        for glob in &config.ignore_globs {
            override_builder.add(&format!("!{glob}")).map_err(|err| {
                Error::Domain(format!("invalid file_picker ignore_glob {glob:?}: {err}"))
            })?;
        }
        let overrides = override_builder
            .build()
            .map_err(|err| Error::Domain(format!("invalid file_picker ignore_globs: {err}")))?;

        let mut walker = ignore::WalkBuilder::new(&root_path);
        walker
            .hidden(!config.include_hidden)
            .git_ignore(config.respect_gitignore)
            .ignore(config.respect_ignore_files)
            .git_exclude(config.respect_git_exclude)
            .follow_links(config.follow_symlinks)
            .overrides(overrides);

        let candidate_limit = config.max_candidates.max(1);
        let result_limit = limit
            .unwrap_or(config.max_results)
            .min(config.max_results)
            .max(1);
        let started = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(config.timeout_ms);
        let normalized_query = query.trim().trim_start_matches('@');
        let mut warnings = Vec::new();
        let mut candidates = Vec::<FilePickerFileView>::new();
        let mut truncated = false;

        for entry in walker.build() {
            if started.elapsed() > timeout {
                truncated = true;
                warnings.push(
                    "file picker search timed out before scanning the whole workspace".to_string(),
                );
                break;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    warnings.push(err.to_string());
                    continue;
                }
            };
            let Some(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_file() && !file_type.is_dir() {
                continue;
            }
            let path = entry.path();
            let relative = match path.strip_prefix(&root_path) {
                Ok(relative) => path_to_api_relative(relative),
                Err(_) => continue,
            };
            if relative.is_empty() {
                continue;
            }
            let kind = if file_type.is_dir() {
                "directory"
            } else {
                "file"
            }
            .to_string();
            let name = Path::new(&relative)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&relative)
                .to_string();
            candidates.push(FilePickerFileView {
                path: relative,
                name,
                kind,
            });
            if candidates.len() >= candidate_limit {
                truncated = true;
                break;
            }
        }

        let pattern = Pattern::new(
            normalized_query,
            CaseMatching::Smart,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
        let files = pattern
            .match_list(candidates, &mut matcher)
            .into_iter()
            .take(result_limit)
            .map(|(entry, _)| entry)
            .collect();

        Ok(FilePickerResultView {
            files,
            truncated,
            warnings,
        })
    }

    fn config_file_picker(&self) -> FilePickerConfig {
        self.file_picker.clone()
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

    pub async fn rename_workspace(
        &self,
        workspace_id: &str,
        request: RenameWorkspaceRequest,
    ) -> Result<WorkspaceView> {
        let name = request
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty());
        let rows_affected = SqliteWorkspaceRepository::new(self.pool.clone())
            .rename_workspace(workspace_id, name)
            .await?;
        if rows_affected == 0 {
            return Err(Error::NotFound(format!(
                "workspace {workspace_id} not found"
            )));
        }
        ExternalQueryService::new(self.pool.clone())
            .get_workspace(workspace_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("workspace {workspace_id} not found")))
    }

    pub async fn delete_workspace(&self, workspace_id: &str) -> Result<WorkspaceView> {
        let rows_affected = SqliteWorkspaceRepository::new(self.pool.clone())
            .mark_deleted(workspace_id)
            .await?;
        if rows_affected == 0 {
            return Err(Error::NotFound(format!(
                "workspace {workspace_id} not found"
            )));
        }
        ExternalQueryService::new(self.pool.clone())
            .get_workspace(workspace_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("workspace {workspace_id} not found")))
    }

    fn root(&self, root_id: &str) -> Result<&WorkspaceRootConfig> {
        self.config
            .roots
            .iter()
            .find(|root| root.root_id == root_id)
            .ok_or_else(|| Error::NotFound(format!("workspace root {root_id} not found")))
    }
}
