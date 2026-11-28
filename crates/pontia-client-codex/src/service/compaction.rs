use super::{CodexService, string};
use crate::runtime::CodexRuntime;
use pontia_agent_control::runtime::ControlTarget;
use pontia_core::{Error, Result, domain::EventType};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

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
            .ok_or_else(|| Error::StateConflict("Codex thread has no persisted history".into()))?;
        if turns.iter().any(|turn| turn["status"] == "inProgress") {
            return Err(Error::StateConflict(
                "Codex has an active native operation".into(),
            ));
        }
        let completed = self
            .compaction_ids(session, EventType::SessionCompactionCompleted)
            .await?;
        let failed = self
            .compaction_ids(session, EventType::SessionCompactionFailed)
            .await?;
        // Save before sending: a lost RPC receipt must not lose manual correlation.
        self.update_compactions(session, |state| {
            if state.pending.is_some() {
                return Err(Error::ControlUnknown(
                    "Codex compaction awaits reconciliation".into(),
                ));
            }
            if state.operations.iter().any(|op| {
                !completed.contains(&op.operation_id) && !failed.contains(&op.operation_id)
            }) {
                return Err(Error::StateConflict(
                    "Codex compaction is already in progress".into(),
                ));
            }
            state.pending = Some(Pending {
                operation_id: operation_id.into(),
                previous_turns: turns
                    .iter()
                    .filter_map(|turn| turn["id"].as_str().map(str::to_owned))
                    .collect(),
            });
            Ok(())
        })
        .await?;
        let result = connection
            .call(
                "thread/compact/start",
                json!({"threadId":binding.client_session_key}),
            )
            .await;
        if matches!(result, Err(Error::StateConflict(_))) {
            self.update_compactions(session, |state| {
                if state
                    .pending
                    .as_ref()
                    .is_some_and(|p| p.operation_id == operation_id)
                {
                    state.pending = None;
                }
                Ok(())
            })
            .await?;
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
        let metadata = self
            .queries()
            .session_metadata(session)
            .await?
            .ok_or_else(Error::missing_record)?;
        Ok(metadata
            .get("codex_compactions")
            .cloned()
            .map(serde_json::from_value)
            .transpose()?
            .unwrap_or_default())
    }

    async fn update_compactions<T>(
        &self,
        session: &str,
        update: impl FnOnce(&mut Compactions) -> Result<T>,
    ) -> Result<T> {
        self.metadata()
            .modify_key(session, "codex_compactions", |old| {
                let mut state: Compactions = old
                    .map(serde_json::from_value)
                    .transpose()?
                    .unwrap_or_default();
                let original = state.clone();
                let result = update(&mut state)?;
                let next = (state != original)
                    .then(|| serde_json::to_value(state))
                    .transpose()?;
                Ok((next, result))
            })
            .await
    }

    async fn compaction_fact(
        &self,
        session: &str,
        operation: &Operation,
        kind: EventType,
        mut payload: Value,
    ) -> Result<()> {
        for existing in [
            EventType::SessionCompactionStarted,
            EventType::SessionCompactionCompleted,
            EventType::SessionCompactionFailed,
        ] {
            if existing == EventType::SessionCompactionStarted && kind != existing {
                continue;
            }
            if self
                .compaction_ids(session, existing)
                .await?
                .contains(&operation.operation_id)
            {
                return Ok(());
            }
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
        let failed = self
            .compaction_ids(session, EventType::SessionCompactionFailed)
            .await?;
        let thread = self.binding(session).await?.client_session_key;
        let operation = self
            .update_compactions(session, |state| {
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
                        let manual = state.pending.as_ref().is_some_and(|pending| {
                            !pending.previous_turns.iter().any(|id| id == turn)
                        });
                        let operation_id = if manual {
                            state.pending.take().unwrap().operation_id
                        } else {
                            // Native item identities are scoped to a native turn and Session.
                            format!("codex_compact:{thread}:{turn}:{id}")
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
                Ok(state.operations[index].clone())
            })
            .await?;
        self.compaction_fact(
            session,
            &operation,
            EventType::SessionCompactionStarted,
            json!({}),
        )
        .await?;
        if completed {
            self.compaction_fact(
                session,
                &operation,
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
        let bound = self.native_turns().is_bound(session, turn).await?;
        self.compaction_item(
            session,
            turn,
            &params["item"],
            event["method"] == "item/completed",
            !bound,
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
        let failed = self
            .compaction_ids(session, EventType::SessionCompactionFailed)
            .await?;
        let checkpoints: Vec<_> = items
            .iter()
            .filter(|item| item["type"] == "contextCompaction")
            .collect();
        let state = self
            .update_compactions(session, |state| {
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
                for (index, item) in checkpoints.iter().enumerate() {
                    if let Some(operation) = state.operations.iter_mut().find(|op| {
                        op.turn == native
                            && op.history_index == index
                            && !failed.contains(&op.operation_id)
                    }) && operation.item.is_none()
                    {
                        operation.item = Some(string(item, "id")?.into());
                    }
                }
                if !checkpoints.is_empty() {
                    let count = state.history_counts.entry(native.into()).or_default();
                    *count = (*count).max(checkpoints.len());
                }
                Ok(state.clone())
            })
            .await?;
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

    async fn compaction_ids(&self, session: &str, kind: EventType) -> Result<HashSet<String>> {
        Ok(self
            .queries()
            .list_events_by_type(session, kind, None)
            .await?
            .into_iter()
            .filter_map(|payload| payload["operation_id"].as_str().map(str::to_owned))
            .collect())
    }
}
