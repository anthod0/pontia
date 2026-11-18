use std::collections::HashSet;

use crate::runtime::{CodexRuntime, protocol::Connection};
use pontia_core::{Error, Result, domain::EventType};
use serde_json::{Value, json};

use super::{CodexService, string};
use pontia_agent_control::agent_clients::AgentClientModels;
use pontia_agent_control::sessions::SessionModel;

impl CodexService {
    pub(crate) async fn available_models(root: &std::path::Path) -> Result<AgentClientModels> {
        let runtime = CodexRuntime::ensure(root).await?;
        let connection = runtime.connection().await?;
        let mut models = list_models(&connection).await?;
        let configuration = connection
            .call("config/read", json!({"cwd":root,"includeLayers":false}))
            .await?;
        let configured: Option<String> =
            serde_json::from_value(configuration["config"]["model"].clone())?;
        if configured.is_some() {
            models.default_model = configured;
        }
        runtime.current_guard().await?;
        Ok(models)
    }

    pub(super) async fn model_fact(&self, session: &str, settings: &Value) -> Result<()> {
        let model = string(settings, "model")?;
        self.event_ingest
            .report_client_fact(
                session,
                None,
                EventType::SessionModelUpdated,
                json!({"model":model}),
            )
            .await
    }

    pub(super) async fn model_snapshot(
        &self,
        session: &str,
        runtime: &CodexRuntime,
        settings: &Value,
    ) -> Result<()> {
        runtime.current_guard().await?;
        self.model_fact(session, settings).await
    }
}

async fn list_models(connection: &Connection) -> Result<AgentClientModels> {
    let mut models = Vec::new();
    let mut default_model = None;
    let mut cursor = Value::Null;
    let mut seen = HashSet::new();
    loop {
        let response = connection
            .call(
                "model/list",
                json!({"limit":100,"cursor":cursor,"includeHidden":false}),
            )
            .await?;
        let page = response["data"]
            .as_array()
            .ok_or_else(|| Error::Domain("Codex model/list has no data".into()))?;
        for model in page {
            if model["hidden"] == true {
                continue;
            }
            if model["isDefault"] == true {
                default_model = Some(string(model, "model")?.into());
            }
            models.push(SessionModel {
                id: string(model, "model")?.into(),
                name: string(model, "displayName")?.into(),
                description: model["description"].as_str().unwrap_or_default().into(),
            });
        }
        cursor = response["nextCursor"].clone();
        if cursor.is_null() {
            break;
        }
        if !cursor.is_string() || !seen.insert(cursor.to_string()) {
            return Err(Error::Domain("Invalid Codex model list cursor".into()));
        }
    }
    Ok(AgentClientModels {
        models,
        default_model,
    })
}

#[cfg(test)]
mod tests;
