use super::{CodexService, string};
use crate::proxy::ThreadOperation;
use pontia_agent_control::{
    AgentBindingService, client_contract::NativeSessionFork, sessions::NativeSessionService,
};
use pontia_core::{Error, Result};
use serde_json::{Value, json};

impl CodexService {
    pub(crate) async fn fork_session(
        &self,
        session: &str,
        turn: &str,
    ) -> Result<NativeSessionFork> {
        let runtime = self.runtime(session).await?;
        let binding = self.binding(session).await?;
        let native: String = sqlx::query_scalar(
            "SELECT client_turn_id FROM native_turn_bindings WHERE session_id=? AND turn_id=?",
        )
        .bind(session)
        .bind(turn)
        .fetch_one(&self.pool)
        .await?;
        let connection = runtime.connection().await?;
        let turns = self
            .turns(&connection, &binding.client_session_key)
            .await?
            .unwrap_or_default();
        if !turns.iter().any(|turn| {
            turn["id"] == native
                && matches!(
                    turn["status"].as_str(),
                    Some("completed" | "failed" | "interrupted")
                )
                && turn["items"]
                    .as_array()
                    .is_some_and(|items| items.iter().any(|item| item["type"] == "userMessage"))
        }) {
            return Err(Error::StateConflict(
                "The native Turn is not an editable completed Turn".into(),
            ));
        }
        let params = json!({"threadId":binding.client_session_key,"beforeTurnId":native,"excludeTurns":true});
        let result = connection.call("thread/fork", params.clone()).await?;
        let native = string(&result["thread"], "id")
            .map_err(|error| Error::ControlUnknown(error.to_string()))?;
        if native == binding.client_session_key {
            return Err(Error::ControlUnknown(
                "Codex fork returned the source thread identity".into(),
            ));
        }
        Ok(NativeSessionFork {
            client_session_key: native.into(),
            receipt: json!({"result":result,"params":params}),
        })
    }

    pub(crate) async fn adopt_fork(
        &self,
        session: &str,
        fork: &NativeSessionFork,
    ) -> Result<String> {
        let runtime = self.runtime(session).await?;
        let connection = runtime.connection().await?;
        let snapshot = connection
            .call(
                "thread/read",
                json!({"threadId":fork.client_session_key,"includeTurns":false}),
            )
            .await?;
        let result = if snapshot["thread"]["status"]["type"] == "notLoaded" {
            // This is an explicit retry of a recorded fork, unlike a queued CLI receipt.
            connection
                .resume_thread(
                    json!({"threadId":fork.client_session_key,"excludeTurns":true}),
                    true,
                )
                .await?
                .0
        } else {
            fork.receipt["result"].clone()
        };
        self.adopt_opened_thread(
            &runtime,
            ThreadOperation::Fork,
            &result,
            &fork.receipt["params"],
            None,
            None,
        )
        .await
        .map(|(session, _operation)| session)
    }

    pub(super) async fn record_fork_lineage(
        &self,
        child: &str,
        thread: &Value,
        params: &Value,
    ) -> Result<()> {
        let Some(parent) = thread["forkedFromId"].as_str().or_else(|| {
            // Only a fork receipt carries source parameters here.
            params["threadId"].as_str().filter(|id| thread["id"] != *id)
        }) else {
            return Ok(());
        };
        let Some(parent_binding) = AgentBindingService::new(self.pool.clone())
            .binding_for_client_session("codex", parent)
            .await?
        else {
            return Ok(());
        };
        let before = params["beforeTurnId"].as_str();
        let last = params["lastTurnId"].as_str();
        let boundary = before.or(last);
        let turn: Option<String> = if let Some(boundary) = boundary {
            sqlx::query_scalar(
                "SELECT turn_id FROM native_turn_bindings WHERE session_id=? AND client_turn_id=?",
            )
            .bind(&parent_binding.session_id)
            .bind(boundary)
            .fetch_optional(&self.pool)
            .await?
        } else {
            None
        };
        NativeSessionService::new(self.pool.clone(), self.event_ingest.clone()).record_fork(
            child, &parent_binding.session_id, turn.as_deref(), boundary,
            json!({"boundary": if before.is_some() { Some("before") } else if last.is_some() { Some("after") } else { None }}),
        ).await
    }
}
