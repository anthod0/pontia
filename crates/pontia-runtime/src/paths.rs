use std::path::{Path, PathBuf};

use pontia_core::error::Result;

use super::RuntimeStartRequest;

pub(super) fn workspace_path(pontia_home: &Path, request: &RuntimeStartRequest) -> Result<PathBuf> {
    if let Some(workspace) = &request.workspace {
        let path = PathBuf::from(workspace);
        if !std::fs::metadata(&path)?.is_dir() {
            return Err(std::io::Error::from(std::io::ErrorKind::NotADirectory).into());
        }
        return Ok(path);
    }
    let path = pontia_home.join("workspaces").join(&request.session_id);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmux_creates_its_default_directory_and_preserves_a_missing_explicit_workspace() {
        let root = tempfile::tempdir().unwrap();
        let mut request: RuntimeStartRequest = serde_json::from_value(serde_json::json!({
            "session_id": "session",
            "client_type": "test"
        }))
        .unwrap();
        let default = workspace_path(root.path(), &request).unwrap();
        assert!(default.is_dir());
        let missing = root.path().join("moved-project");
        request.workspace = Some(missing.display().to_string());
        assert!(workspace_path(root.path(), &request).is_err());
        assert!(!missing.exists());
    }
}
