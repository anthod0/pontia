mod commands;
pub mod path_policy;
pub use commands::{RegisterWorkspaceRequest, WORKSPACE_DESCRIPTION_LIMIT, WorkspaceService};
pub mod availability;
mod persistence;
pub(crate) use persistence::resolve_session_directory;
pub use persistence::{
    WorkspaceRegistration, get_workspace_record, require_active_workspace,
    require_active_workspace_id, upsert_canonical_workspace,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRecord {
    pub workspace_id: String,
    pub canonical_path: String,
    pub name: Option<String>,
    pub description: Option<String>,
}
