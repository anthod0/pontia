mod commands;
pub mod path_policy;
pub use commands::{RegisterWorkspaceRequest, WORKSPACE_DESCRIPTION_LIMIT, WorkspaceService};
pub mod availability;
mod persistence;
pub use persistence::{
    WorkspaceRegistration, get_workspace_record, require_active_workspace,
    require_active_workspace_id, upsert_canonical_workspace,
};
pub(crate) use persistence::{require_managed_launch_directory, resolve_session_directory};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRecord {
    pub workspace_id: String,
    pub canonical_path: String,
    pub name: Option<String>,
    pub description: Option<String>,
}
