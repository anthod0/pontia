use std::{path::Path, process::Stdio, time::Duration};

use pontia_agent_control::agent_clients::AgentClientModels;
use pontia_core::{Error, Result};

pub(crate) async fn list(script: &Path, directory: &Path) -> Result<AgentClientModels> {
    let operation = async {
        let output = tokio::process::Command::new(crate::setup::node())
            .arg(script)
            .arg("models")
            .current_dir(directory)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output()
            .await?;
        if !output.status.success() {
            return Err(Error::Unavailable(
                "Claude Code model listing failed".into(),
            ));
        }
        Ok(AgentClientModels {
            models: serde_json::from_slice(&output.stdout)?,
            default_model: None,
        })
    };
    tokio::time::timeout(Duration::from_secs(30), operation)
        .await
        .map_err(|_| Error::Unavailable("Claude Code model listing timed out".into()))?
}
