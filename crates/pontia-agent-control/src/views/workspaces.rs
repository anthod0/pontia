use serde::Serialize;
use serde_json::Value;

use pontia_core::error::Result;
use pontia_storage_sqlite::models::workspaces::WorkspaceRow;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WorkspaceView {
    pub workspace_id: String,
    pub canonical_path: String,
    pub display_path: String,
    pub name: Option<String>,
    pub state: String,
    pub availability: WorkspaceAvailability,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum WorkspaceAvailability {
    Available { root_id: String, path: String },
    Unavailable { reason: WorkspaceUnavailableReason },
    Unknown { reason: WorkspaceCheckFailure },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceUnavailableReason {
    OutsideRoot,
    MissingDirectory,
    NotDirectory,
    PermissionDenied,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceCheckFailure {
    RootUnavailable,
    CheckFailed,
}

pub(crate) fn row_to_view(
    row: WorkspaceRow,
    availability: WorkspaceAvailability,
) -> Result<WorkspaceView> {
    Ok(WorkspaceView {
        workspace_id: row.workspace_id,
        canonical_path: row.canonical_path,
        display_path: row.display_path,
        name: row.name,
        state: row.state,
        availability,
        metadata: serde_json::from_str(&row.metadata)?,
        created_at: row.created_at,
        updated_at: row.updated_at,
        last_used_at: row.last_used_at,
    })
}
