mod browser;
mod path_policy;
mod types;

pub use pontia_config::{FilePickerConfig, WorkspaceBrowserConfig, WorkspaceRootConfig};

pub use browser::WorkspaceBrowserService;
pub use types::{
    FilePickerFileView, FilePickerResultView, RegisterWorkspaceRequest,
    WorkspaceDirectoryEntryView, WorkspaceDirectoryListingView, WorkspaceRootView,
};

pub(crate) async fn accessible_workspace_path(
    pool: &sqlx::SqlitePool,
    config: &WorkspaceBrowserConfig,
    workspace_id: &str,
) -> pontia_core::Result<std::path::PathBuf> {
    let workspace =
        pontia_agent_control::workspaces::require_active_workspace_id(pool, workspace_id).await?;
    let path = std::fs::canonicalize(workspace.canonical_path)?;
    if path.is_dir()
        && config
            .roots
            .iter()
            .filter_map(|root| path_policy::canonical_root(root).ok())
            .any(|root| path.starts_with(root))
    {
        Ok(path)
    } else {
        Err(pontia_core::Error::NotFound(format!(
            "workspace {workspace_id} directory is not available"
        )))
    }
}
