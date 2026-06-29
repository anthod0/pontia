use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FilePickerFileView {
    pub path: String,
    pub name: String,
    pub kind: String,
}

impl AsRef<str> for FilePickerFileView {
    fn as_ref(&self) -> &str {
        &self.path
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FilePickerResultView {
    pub files: Vec<FilePickerFileView>,
    pub truncated: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RegisterWorkspaceRequest {
    pub root_id: String,
    #[serde(default)]
    pub path: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RenameWorkspaceRequest {
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRecord {
    pub workspace_id: String,
    pub canonical_path: String,
}
