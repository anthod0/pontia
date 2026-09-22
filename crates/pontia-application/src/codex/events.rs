use super::{CodexService, string};
use crate::{EventReportError, ExternalQueryService, PontiaEventType, ReportedFact};
use pontia_core::domain::EventType;
use pontia_core::{Error, Result};
use pontia_runtime::codex::{CodexRuntime, protocol::Connection};
use serde_json::{Value, json};
use std::collections::HashSet;

impl CodexService {
    pub(super) async fn report(
        &self,
        session: &str,
        runtime_instance_id: &str,
        kind: EventType,
        mut data: Value,
    ) -> Result<()> {
        data["runtime_instance_id"] = json!(runtime_instance_id);
        self.event_ingest
            .report_fact(ReportedFact {
                session_id: session.into(),
                turn_id: None,
                fact_type: kind,
                data,
            })
            .await
            .map_err(|error| match error {
                EventReportError::InvalidFact(message) => Error::Domain(message),
                EventReportError::Ingestion(error) => error,
            })?;
        Ok(())
    }

    pub(super) async fn ready(
        &self,
        session: &str,
        runtime: &CodexRuntime,
        thread: &Value,
    ) -> Result<()> {
        if thread["canAcceptDirectInput"].as_bool() != Some(true)
            || !matches!(
                thread.pointer("/status/type").and_then(Value::as_str),
                // Native systemError records the last failure; this version still
                // accepts new input when canAcceptDirectInput explicitly says so.
                Some("idle" | "active" | "systemError")
            )
        {
            return Err(Error::CapabilityUnavailable(
                "Codex thread is not ready to accept input".into(),
            ));
        }
        let state = ExternalQueryService::new(self.pool.clone())
            .get_session(session)
            .await?
            .ok_or_else(|| Error::NotFound(session.into()))?;
        if state.state == "exited" {
            self.owned_event(session, PontiaEventType::SessionResuming)
                .await?;
        }
        let already: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE session_id=? AND event_type='session.ready' AND json_extract(payload,'$.runtime_instance_id')=?")
            .bind(session).bind(&runtime.instance_id).fetch_one(&self.pool).await?;
        if already == 0 || state.state == "exited" {
            self.report(session,&runtime.instance_id,EventType::SessionReady,json!({"client_session_key":thread["id"],"launch_cwd":thread["cwd"],"client_session_file":thread["path"]})).await?;
        }
        Ok(())
    }

    pub(super) async fn turns(&self, connection: &Connection, thread: &str) -> Result<Vec<Value>> {
        let mut cursor = Value::Null;
        let mut seen = HashSet::new();
        let mut turns = Vec::new();
        loop {
            let response = connection.call("thread/turns/list",json!({"threadId":thread,"cursor":cursor,"limit":100,"sortDirection":"asc","itemsView":"full"})).await?;
            let page = response["data"]
                .as_array()
                .ok_or_else(|| Error::Domain("Codex turns/list has no data".into()))?;
            turns.extend(page.iter().cloned());
            cursor = response["nextCursor"].clone();
            if cursor.is_null() {
                break;
            }
            if !seen.insert(cursor.to_string()) {
                return Err(Error::Domain("Codex repeated history cursor".into()));
            }
        }
        Ok(turns)
    }

    pub(super) async fn reconcile_turns(
        &self,
        session: &str,
        runtime: &CodexRuntime,
        turns: &[Value],
    ) -> Result<()> {
        for turn in turns {
            self.turn_fact(session, &runtime.instance_id, turn, "snapshot")
                .await?;
        }
        Ok(())
    }

    pub(super) async fn turn_fact(
        &self,
        session: &str,
        runtime_instance_id: &str,
        turn: &Value,
        origin: &str,
    ) -> Result<()> {
        let native_id = string(turn, "id")?;
        let existing: Option<(String,String)> = sqlx::query_as("SELECT t.turn_id,t.state FROM native_turn_bindings b JOIN turns t ON t.turn_id=b.turn_id WHERE b.session_id=? AND b.client_turn_id=?")
            .bind(session).bind(native_id).fetch_optional(&self.pool).await?;
        if existing.as_ref().is_some_and(|(_, state)| {
            matches!(state.as_str(), "completed" | "failed" | "interrupted")
        }) {
            return Ok(());
        }
        let items = turn["items"].as_array().cloned().unwrap_or_default();
        if existing.is_none() {
            let input = items
                .iter()
                .find(|item| item["type"] == "userMessage")
                .and_then(|item| item["content"].as_array())
                .map(|content| {
                    content
                        .iter()
                        .filter_map(|part| part["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                });
            let dispatch: Option<(String,String)> = sqlx::query_as("SELECT message_id,input_summary FROM inbox_messages WHERE session_id=? AND json_extract(metadata,'$.codex_turn_id')=? ORDER BY created_at LIMIT 1").bind(session).bind(native_id).fetch_optional(&self.pool).await?;
            let summary = input
                .as_deref()
                .or_else(|| dispatch.as_ref().map(|(_, input)| input.as_str()));
            self.report(session,runtime_instance_id,EventType::TurnStarted,json!({"native_turn_id":native_id,"input":{"summary":bounded(summary)},"metadata":{"native_turn_id":native_id,"native_started_at":turn["startedAt"],"observation":origin,"inbox_message_id":dispatch.map(|(id,_)|id)}})).await?;
        }
        sqlx::query("UPDATE inbox_messages SET turn_id=(SELECT turn_id FROM native_turn_bindings WHERE session_id=? AND client_turn_id=?) WHERE session_id=? AND json_extract(metadata,'$.codex_turn_id')=? AND turn_id IS NULL")
            .bind(session).bind(native_id).bind(session).bind(native_id).execute(&self.pool).await?;
        let kind = match turn["status"].as_str() {
            Some("completed") => EventType::TurnCompleted,
            Some("failed") => EventType::TurnFailed,
            Some("interrupted") => EventType::TurnInterrupted,
            Some("inProgress") => return Ok(()),
            _ => return Err(Error::Domain("Unknown Codex turn status".into())),
        };
        let final_message = items
            .iter()
            .rev()
            .find(|item| item["type"] == "agentMessage" && item["phase"] == "final_answer")
            .or_else(|| {
                items
                    .iter()
                    .rev()
                    .find(|item| item["type"] == "agentMessage" && item["phase"].is_null())
            });
        if let Some(text) = final_message.and_then(|item| item["text"].as_str()) {
            self.report(
                session,
                runtime_instance_id,
                EventType::TurnOutput,
                json!({"native_turn_id":native_id,"output":{"summary":bounded(Some(text))}}),
            )
            .await?;
        }
        self.report(session,runtime_instance_id,kind,json!({"native_turn_id":native_id,"native_completed_at":turn["completedAt"],"observation":origin,"failure":{"message":bounded(turn.pointer("/error/message").and_then(Value::as_str))}})).await
    }

    pub(super) async fn check_archived(
        &self,
        session: &str,
        runtime: &CodexRuntime,
        thread: &str,
    ) -> Result<bool> {
        let connection = runtime.connection().await?;
        let mut cursor = Value::Null;
        let mut seen = HashSet::new();
        loop {
            let response = connection
                .call(
                    "thread/list",
                    json!({"archived":true,"cursor":cursor,"limit":100,"sourceKinds":[]}),
                )
                .await?;
            if response["data"]
                .as_array()
                .is_some_and(|threads| threads.iter().any(|item| item["id"] == thread))
            {
                self.archived(session, runtime).await?;
                return Ok(true);
            }
            cursor = response["nextCursor"].clone();
            if cursor.is_null() {
                return Ok(false);
            }
            if !seen.insert(cursor.to_string()) {
                return Err(Error::Domain("Codex repeated thread list cursor".into()));
            }
        }
    }

    pub(super) async fn archived(&self, session: &str, runtime: &CodexRuntime) -> Result<()> {
        if let Some(binding) = crate::AgentBindingService::new(self.pool.clone())
            .binding_for_session(session)
            .await?
        {
            let snapshot = runtime
                .connection()
                .await?
                .call(
                    "thread/read",
                    json!({"threadId":binding.client_session_key,"includeTurns":false}),
                )
                .await?;
            self.bind(session, runtime, &snapshot["thread"]).await?;
            let connection = runtime.connection().await?;
            let turns = self.turns(&connection, &binding.client_session_key).await?;
            self.reconcile_turns(session, runtime, &turns).await?;
        }
        let state: String = sqlx::query_scalar("SELECT state FROM sessions WHERE session_id=?")
            .bind(session)
            .fetch_one(&self.pool)
            .await?;
        if state != "exited" {
            self.report(
                session,
                &runtime.instance_id,
                EventType::SessionExited,
                json!({"reason":"thread_archived"}),
            )
            .await?;
        }
        self.connection_state(session, &runtime.instance_id, "archived")
            .await
    }
}

fn bounded(text: Option<&str>) -> Option<String> {
    text.map(|text| text.chars().take(200).collect())
}
