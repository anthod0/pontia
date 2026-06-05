use std::path::PathBuf;

use crate::error::{Error, Result};

use super::RuntimeStartRequest;

pub(super) fn workspace_path(request: &RuntimeStartRequest) -> Result<PathBuf> {
    let path = request
        .workspace
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir()
                .join("pilotfy-workspaces")
                .join(&request.session_id)
        });
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

pub(super) fn runtime_dir(session_id: &str) -> Result<PathBuf> {
    Ok(pilotfy_data_dir()?.join("runtimes").join(session_id))
}

fn pilotfy_data_dir() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("PILOTFY_DATA_DIR") {
        return Ok(PathBuf::from(path));
    }

    let home = std::env::var("HOME").map_err(|_| Error::InvalidConfig {
        key: "HOME",
        message: "required to derive pilotfy data directory".to_string(),
    })?;
    Ok(PathBuf::from(home).join(".local/share/pilotfy"))
}
