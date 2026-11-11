pub mod availability;
mod persistence;
pub use persistence::{get_workspace_record, upsert_canonical_workspace, upsert_workspace};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRecord {
    pub workspace_id: String,
    pub canonical_path: String,
    pub name: Option<String>,
}
