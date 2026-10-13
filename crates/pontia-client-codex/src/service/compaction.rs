use super::{CodexService, string};
use crate::runtime::CodexRuntime;
use pontia_application::runtime::ControlTarget;
use pontia_core::{Error, Result, domain::EventType};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

// Only correlation metadata belongs here; native content remains in Codex history.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Compactions {
    pending: Option<Pending>,
    operations: Vec<Operation>,
    history_counts: BTreeMap<String, usize>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Pending {
    operation_id: String,
    previous_turns: Vec<String>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Operation {
    operation_id: String,
    trigger: String,
    turn: String,
    item: Option<String>,
    standalone: bool,
    history_index: usize,
}

impl CodexService {
    pub(crate) async fn compact(&self, target: &ControlTarget, operation_id: &str) -> Result<()> {
        let session = &target.session_id;
        let runtime = self.runtime(session).await?;
        let _operation = runtime.lock_session(session).await;
        self.confirm_control_connection(target, &runtime).await?;
        let binding = self.binding(session).await?;
        let connection = runtime.connection().await?;
        let turns = self
            .turns(&connection, &binding.client_session_key)
            .await?
            .ok_or_else(|| {
                Error::CapabilityUnavailable("Codex thread has no persisted history".into())
            })?;
        if turns.iter().any(|turn| turn["status"] == "inProgress") {
            return Err(Error::StateConflict(
                "Codex has an active native operation".into(),
            ));
        }
        let mut state = self.compactions(session).await?;
        if state.pending.is_some() {
            return Err(Error::ControlUnknown(
                "Codex compaction awaits reconciliation".into(),
            ));
        }
        for operation in &state.operations {
            if !self
                .compaction_terminal(session, &operation.operation_id)
                .await?
            {
                return Err(Error::StateConflict(
                    "Codex compaction is already in progress".into(),
                ));
            }
        }
        state.pending = Some(Pending {
            operation_id: operation_id.into(),
            previous_turns: turns
                .iter()
                .filter_map(|turn| turn["id"].as_str().map(str::to_owned))
                .collect(),
        });
        // Save before sending: a lost RPC receipt must not lose manual correlation.
        self.save_compactions(session, &state).await?;
        let result = connection
            .call(
                "thread/compact/start",
                json!({"threadId":binding.client_session_key}),
            )
            .await;
        if matches!(result, Err(Error::StateConflict(_))) {
            state.pending = None;
            self.save_compactions(session, &state).await?;
        }
        runtime.current_guard().await?;
        let response = result?;
        if response.as_object().is_none_or(|value| !value.is_empty()) {
            return Err(Error::ControlUnknown(
                "Invalid Codex compaction acknowledgement".into(),
            ));
        }
        Ok(())
    }

    async fn compactions(&self, session: &str) -> Result<Compactions> {
        let value: Option<String> = sqlx::query_scalar(
            "SELECT json_extract(metadata,'$.codex_compactions') FROM sessions WHERE session_id=?",
        )
        .bind(session)
        .fetch_one(&self.pool)
        .await?;
        Ok(value
            .map(|value| serde_json::from_str(&value))
            .transpose()?
            .unwrap_or_default())
    }

    async fn save_compactions(&self, session: &str, state: &Compactions) -> Result<()> {
        sqlx::query("UPDATE sessions SET metadata=json_set(metadata,'$.codex_compactions',json(?)) WHERE session_id=?")
            .bind(serde_json::to_string(state)?).bind(session).execute(&self.pool).await?;
        Ok(())
    }

    async fn compaction_terminal(&self, session: &str, operation: &str) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM events WHERE session_id=? AND json_extract(payload,'$.operation_id')=? AND event_type IN ('session.compaction_completed','session.compaction_failed')")
            .bind(session).bind(operation).fetch_one(&self.pool).await? > 0)
    }

    async fn compaction_fact(
        &self,
        session: &str,
        operation: &Operation,
        kind: EventType,
        mut payload: Value,
    ) -> Result<()> {
        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE session_id=? AND event_type=? AND json_extract(payload,'$.operation_id')=?")
            .bind(session).bind(kind.to_string()).bind(&operation.operation_id).fetch_one(&self.pool).await?;
        if exists > 0
            || self
                .compaction_terminal(session, &operation.operation_id)
                .await?
        {
            return Ok(());
        }
        payload["operation_id"] = json!(operation.operation_id);
        payload["trigger"] = json!(operation.trigger);
        self.event_ingest
            .report_client_fact(session, None, kind, payload)
            .await
    }

    // The caller holds the Session operation gate, including during snapshot recovery.
    pub(super) async fn compaction_item(
        &self,
        session: &str,
        turn: &str,
        item: &Value,
        completed: bool,
        standalone: bool,
    ) -> Result<()> {
        let id = string(item, "id")?;
        let mut state = self.compactions(session).await?;
        let original = state.clone();
        let failed = self.failed_compactions(session).await?;
        let history_index = state
            .operations
            .iter()
            .filter(|op| op.turn == turn && !failed.contains(&op.operation_id))
            .map(|op| op.history_index + 1)
            .max()
            .unwrap_or_default()
            .max(state.history_counts.get(turn).copied().unwrap_or_default());
        let index = match state.operations.iter().position(|op| {
            op.turn == turn
                && (op.item.as_deref() == Some(id)
                    || op.item.is_none()
                    || (op.trigger == "manual" && op.standalone))
        }) {
            Some(index) => index,
            None => {
                let manual = state
                    .pending
                    .as_ref()
                    .is_some_and(|pending| !pending.previous_turns.iter().any(|id| id == turn));
                let operation_id = if manual {
                    state.pending.take().unwrap().operation_id
                } else {
                    // Native item identities are scoped to a native turn and Session.
                    format!(
                        "codex_compact:{}:{turn}:{id}",
                        self.binding(session).await?.client_session_key
                    )
                };
                state.operations.push(Operation {
                    operation_id,
                    trigger: if manual { "manual" } else { "unknown" }.into(),
                    turn: turn.into(),
                    item: None,
                    standalone: standalone || manual,
                    history_index,
                });
                state.operations.len() - 1
            }
        };
        state.operations[index].item = Some(id.into());
        if state != original {
            self.save_compactions(session, &state).await?;
        }
        let operation = &state.operations[index];
        self.compaction_fact(
            session,
            operation,
            EventType::SessionCompactionStarted,
            json!({}),
        )
        .await?;
        if completed {
            self.compaction_fact(
                session,
                operation,
                EventType::SessionCompactionCompleted,
                json!({"native_compaction_id":id,"tokens_before":null,"tokens_after":null}),
            )
            .await?;
        }
        Ok(())
    }

    pub(super) async fn compaction_notification(
        &self,
        runtime: &CodexRuntime,
        session: &str,
        event: &crate::runtime::protocol::Notification,
    ) -> Result<()> {
        let _operation = runtime.lock_session(session).await;
        let _current = runtime.current_guard().await?;
        let params = &event["params"];
        let turn = string(params, "turnId")?;
        let bound: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM native_turn_bindings WHERE session_id=? AND client_turn_id=?",
        )
        .bind(session)
        .bind(turn)
        .fetch_one(&self.pool)
        .await?;
        self.compaction_item(
            session,
            turn,
            &params["item"],
            event["method"] == "item/completed",
            bound == 0,
        )
        .await
    }

    pub(super) async fn compaction_error(
        &self,
        runtime: &CodexRuntime,
        session: &str,
        params: &Value,
    ) -> Result<()> {
        if params["willRetry"].as_bool() != Some(false) {
            return Ok(());
        }
        let _operation = runtime.lock_session(session).await;
        let _current = runtime.current_guard().await?;
        self.fail_compactions(session, string(params, "turnId")?, false, &params["error"])
            .await
    }

    async fn fail_compactions(
        &self,
        session: &str,
        turn: &str,
        aborted: bool,
        error: &Value,
    ) -> Result<()> {
        for operation in self
            .compactions(session)
            .await?
            .operations
            .iter()
            .filter(|op| op.turn == turn)
        {
            self.compaction_fact(
                session,
                operation,
                EventType::SessionCompactionFailed,
                json!({"aborted":aborted,"error_message":error["message"].as_str()}),
            )
            .await?;
        }
        Ok(())
    }

    pub(super) async fn reconcile_compaction_turn(
        &self,
        session: &str,
        turn: &Value,
    ) -> Result<bool> {
        let native = string(turn, "id")?;
        let items = turn["items"].as_array().cloned().unwrap_or_default();
        let standalone =
            !items.is_empty() && items.iter().all(|item| item["type"] == "contextCompaction");
        let mut state = self.compactions(session).await?;
        let original = state.clone();
        if state
            .pending
            .as_ref()
            .is_some_and(|pending| !pending.previous_turns.iter().any(|id| id == native))
            && (items.is_empty() || standalone)
        {
            let pending = state.pending.take().unwrap();
            state.operations.push(Operation {
                operation_id: pending.operation_id,
                trigger: "manual".into(),
                turn: native.into(),
                item: None,
                standalone: true,
                history_index: 0,
            });
        }
        // Automatic pre-turn compaction can precede the user input. Once native
        // execution items exist, its Turn belongs to that execution.
        if items.iter().any(|item| item["type"] != "contextCompaction") {
            for operation in state
                .operations
                .iter_mut()
                .filter(|op| op.turn == native && op.trigger != "manual")
            {
                operation.standalone = false;
            }
        }
        let failed = self.failed_compactions(session).await?;
        let checkpoints: Vec<_> = items
            .iter()
            .filter(|item| item["type"] == "contextCompaction")
            .collect();
        for (index, item) in checkpoints.iter().enumerate() {
            if let Some(operation) = state.operations.iter_mut().find(|op| {
                op.turn == native && op.history_index == index && !failed.contains(&op.operation_id)
            }) && operation.item.is_none()
            {
                operation.item = Some(string(item, "id")?.into());
            }
        }
        if !checkpoints.is_empty() {
            let count = state.history_counts.entry(native.into()).or_default();
            *count = (*count).max(checkpoints.len());
        }
        if state != original {
            self.save_compactions(session, &state).await?;
        }
        // Only recover known live operations or a pending Pontia command. Legacy
        // history has no live item identity, so unknown offline markers remain
        // checkpoints instead of generating synthetic lifecycle operations.
        for (index, item) in checkpoints.iter().enumerate() {
            if let Some(operation) = state.operations.iter().find(|op| {
                op.turn == native && op.history_index == index && !failed.contains(&op.operation_id)
            }) {
                self.compaction_fact(
                    session,
                    operation,
                    EventType::SessionCompactionStarted,
                    json!({}),
                )
                .await?;
                self.compaction_fact(session, operation, EventType::SessionCompactionCompleted,
                    json!({"native_compaction_id":string(item, "id")?,"tokens_before":null,"tokens_after":null})).await?;
            }
        }
        if matches!(turn["status"].as_str(), Some("failed" | "interrupted")) {
            self.fail_compactions(
                session,
                native,
                turn["status"] == "interrupted",
                &turn["error"],
            )
            .await?;
        }
        Ok(standalone
            || state
                .operations
                .iter()
                .any(|op| op.turn == native && op.standalone))
    }

    async fn failed_compactions(&self, session: &str) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar("SELECT json_extract(payload,'$.operation_id') FROM events WHERE session_id=? AND event_type='session.compaction_failed'")
            .bind(session).fetch_all(&self.pool).await?)
    }
}
