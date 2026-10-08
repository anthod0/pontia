use std::collections::HashSet;

use crate::runtime::{CodexRuntime, SubscriptionState, protocol::Connection};
use pontia_core::{Error, Result, domain::EventType};
use serde_json::{Value, json};

use super::{CodexService, string};
use pontia_application::{AgentBindingService, runtime::ControlTarget, sessions::SessionModel};

impl CodexService {
    pub(crate) async fn list_models(&self, target: &ControlTarget) -> Result<Vec<SessionModel>> {
        let runtime = self.runtime(&target.session_id).await?;
        let _operation = runtime.lock_session(&target.session_id).await;
        self.model_target(target, &runtime).await?;
        let connection = runtime.connection().await?;
        let models = list_models(&connection).await?;
        runtime.current_guard().await?;
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
        update_model(&connection, &thread, model).await?;
        runtime.current_guard().await?;
        Ok(())
    }

    async fn model_target(&self, target: &ControlTarget, runtime: &CodexRuntime) -> Result<String> {
        self.confirm_control_connection(target, runtime).await?;
        if !self.session_accepts_control(&target.session_id).await?
            || runtime.subscription(&target.session_id).await != Some(SubscriptionState::Available)
        {
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
