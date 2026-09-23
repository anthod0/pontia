use std::collections::HashSet;

use crate::runtime::{CodexRuntime, protocol::Connection};
use pontia_core::{Error, Result, domain::EventType};
use serde_json::{Value, json};

use super::{CodexService, string};
use pontia_application::{
    AgentBindingService, runtime::control_target::ControlTarget, sessions::SessionModel,
};

impl CodexService {
    pub(crate) async fn list_models(&self, target: &ControlTarget) -> Result<Vec<SessionModel>> {
        let runtime = self.runtime(&target.session_id).await?;
        let _operation = runtime.lock_session(&target.session_id).await;
        self.model_target(target, &runtime).await?;
        let connection = runtime.connection().await?;
        let models = list_models(&connection).await?;
        target.validate(&self.pool).await?;
        Ok(models)
    }

    pub(crate) async fn set_model(&self, target: &ControlTarget, model: &str) -> Result<()> {
        let runtime = self.runtime(&target.session_id).await?;
        let _operation = runtime.lock_session(&target.session_id).await;
        let thread = self.model_target(target, &runtime).await?;
        let connection = runtime.connection().await?;
        if !list_models(&connection)
            .await?
            .iter()
            .any(|entry| entry.id == model)
        {
            return Err(Error::Domain("The selected model is not available.".into()));
        }
        self.model_target(target, &runtime).await?;
        update_model(&connection, &thread, model).await?;
        target
            .validate(&self.pool)
            .await
            .map_err(|error| Error::ControlUnknown(error.to_string()))?;
        // The acknowledgement is not a model observation. The observer ingests
        // thread/settings/updated, including changes made by the native TUI.
        Ok(())
    }

    async fn model_target(&self, target: &ControlTarget, runtime: &CodexRuntime) -> Result<String> {
        target.validate(&self.pool).await?;
        if target.instance()? != runtime.instance_id {
            return Err(Error::StateConflict(
                "Codex runtime requires reconciliation before model control".into(),
            ));
        }
        let available: bool = sqlx::query_scalar("SELECT s.state IN ('idle','busy') AND json_extract(r.adapter_details,'$.codex.connection')='available' FROM sessions s JOIN runtime_bindings r USING(session_id) WHERE s.session_id=?")
            .bind(&target.session_id).fetch_one(&self.pool).await?;
        if !available {
            return Err(Error::CapabilityUnavailable(
                "Codex model control is unavailable".into(),
            ));
        }
        AgentBindingService::new(self.pool.clone())
            .binding_for_session(&target.session_id)
            .await?
            .map(|binding| binding.client_session_key)
            .ok_or_else(|| {
                Error::CapabilityUnavailable("Codex session has no native thread".into())
            })
    }

    pub(super) async fn model_fact(
        &self,
        session: &str,
        runtime_instance_id: &str,
        settings: &Value,
    ) -> Result<()> {
        let model = string(settings, "model")?;
        self.report(
            session,
            runtime_instance_id,
            EventType::SessionModelUpdated,
            json!({"model":model}),
        )
        .await
    }
}

async fn update_model(connection: &Connection, thread: &str, model: &str) -> Result<()> {
    let response = connection
        .call(
            "thread/settings/update",
            json!({"threadId":thread,"model":model}),
        )
        .await?;
    if response != json!({}) {
        return Err(Error::ControlUnknown(
            "Invalid Codex model change acknowledgement".into(),
        ));
    }
    Ok(())
}

async fn list_models(connection: &Connection) -> Result<Vec<SessionModel>> {
    let mut models = Vec::new();
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
    Ok(models)
}

#[cfg(test)]
mod tests;
