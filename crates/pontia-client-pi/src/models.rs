use std::{collections::HashSet, path::Path, process::Stdio, time::Duration};

use pontia_agent_control::{agent_clients::AgentClientModels, sessions::SessionModel};
use pontia_core::{Error, Result};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncWriteExt, BufReader},
    process::{ChildStdin, ChildStdout, Command},
};

#[derive(Deserialize)]
struct Model {
    provider: String,
    id: String,
    name: String,
}

impl Model {
    fn qualified_id(&self) -> Result<String> {
        if self.provider.trim().is_empty() || self.id.trim().is_empty() {
            return Err(Error::Domain("Invalid Pi model identifiers".into()));
        }
        Ok(format!("{}/{}", self.provider, self.id))
    }
}

pub(crate) async fn list(command: &str, directory: &Path) -> Result<AgentClientModels> {
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
        let response = request(&mut stdin, &mut stdout, "get_available_models").await?;
        let models: Vec<Model> = serde_json::from_value(response["models"].clone())?;
        let mut ids = HashSet::new();
        let models = models
            .into_iter()
            .map(|model| {
                let id = model.qualified_id()?;
                if !ids.insert(id.clone()) {
                    return Err(Error::Domain("Invalid Pi model identifiers".into()));
                }
                Ok(SessionModel {
                    id,
                    name: model.name,
                    description: model.provider,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let state = request(&mut stdin, &mut stdout, "get_state").await?;
        let default_model: Option<Model> = serde_json::from_value(state["model"].clone())?;
        let default_model = default_model
            .map(|model| model.qualified_id())
            .transpose()?;
        drop(stdin);
        if !child.wait().await?.success() {
            return Err(Error::CapabilityUnavailable(
                "Pi model listing process failed".into(),
            ));
        }
        Ok(AgentClientModels {
            models,
            default_model,
        })
    };
    tokio::time::timeout(Duration::from_secs(30), operation)
        .await
        .map_err(|_| Error::CapabilityUnavailable("Pi model listing timed out".into()))?
}

async fn request(
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
    command: &str,
) -> Result<Value> {
    let message = format!("{}\n", json!({"id":command,"type":command}));
    stdin.write_all(message.as_bytes()).await?;
    loop {
        let frame = pontia_agent_control::ipc::rpc::read_frame(stdout).await?;
        let response: Value = serde_json::from_slice(&frame)?;
        if response["type"] != "response" || response["id"] != command {
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
        return Ok(response["data"].clone());
    }
}
