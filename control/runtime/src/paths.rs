use std::path::{Path, PathBuf};

use pontia_core::error::Result;

use super::RuntimeStartRequest;

pub(super) fn workspace_path(pontia_home: &Path, request: &RuntimeStartRequest) -> Result<PathBuf> {
    let path = match request.workspace.as_ref() {
        Some(workspace) => PathBuf::from(workspace),
        None => pontia_home.join("workspaces").join(&request.session_id),
    };
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

pub(super) struct LogPaths {
    pub(super) log_dir: PathBuf,
    pub(super) runtime_log: PathBuf,
}

impl LogPaths {
    pub(super) fn client_hook_log(&self, file_name: &str) -> PathBuf {
        self.log_dir.join(file_name)
    }
}

pub(super) fn log_paths(pontia_home: &Path) -> LogPaths {
    let log_dir = pontia_home.join("state");
    LogPaths {
        runtime_log: log_dir.join("runtime.log"),
        log_dir,
    }
}
