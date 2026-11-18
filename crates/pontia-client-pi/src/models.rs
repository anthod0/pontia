use std::{collections::HashSet, path::Path, process::Stdio, time::Duration};

use pontia_agent_control::sessions::SessionModel;
use pontia_core::{Error, Result};
use serde::Deserialize;
use serde_json::Value;
use tokio::{
    io::{AsyncWriteExt, BufReader},
    process::Command,
};

#[derive(Deserialize)]
struct Model {
    provider: String,
    id: String,
    name: String,
}

pub(crate) async fn list(command: &str, directory: &Path) -> Result<Vec<SessionModel>> {
    let operation = async {
        let mut child = Command::new("bash")
            .arg("-c")
            .arg(format!("{command} --mode rpc --no-session"))
            .current_dir(directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let mut stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        stdin
            .write_all(b"{\"id\":\"pontia-models\",\"type\":\"get_available_models\"}\n")
            .await?;
        loop {
            let frame = crate::rpc::read_frame(&mut stdout).await?;
            let response: Value = serde_json::from_slice(&frame)?;
            if response["type"] != "response" || response["id"] != "pontia-models" {
                continue;
            }
            if response["success"] != true {
                return Err(Error::CapabilityUnavailable(
                    response["error"]
                        .as_str()
                        .unwrap_or("Pi model listing failed")
                        .into(),
                ));
            }
            let models: Vec<Model> = serde_json::from_value(response["data"]["models"].clone())?;
            let mut ids = HashSet::new();
            let models = models
                .into_iter()
                .map(|model| {
                    let id = format!("{}/{}", model.provider, model.id);
                    if model.provider.trim().is_empty()
                        || model.id.trim().is_empty()
                        || !ids.insert(id.clone())
                    {
                        return Err(Error::Domain("Invalid Pi model identifiers".into()));
                    }
                    Ok(SessionModel {
                        id,
                        name: model.name,
                        description: model.provider,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            drop(stdin);
            if !child.wait().await?.success() {
                return Err(Error::CapabilityUnavailable(
                    "Pi model listing process failed".into(),
                ));
            }
            return Ok(models);
        }
    };
    tokio::time::timeout(Duration::from_secs(30), operation)
        .await
        .map_err(|_| Error::CapabilityUnavailable("Pi model listing timed out".into()))?
}
