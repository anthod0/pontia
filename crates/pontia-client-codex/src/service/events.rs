use super::{CodexService, string};
use crate::runtime::{CodexRuntime, protocol::Connection};
use pontia_application::{
    EventReportError, ReportedFact,
    sessions::NativeSessionService,
    turns::{NativeTurnObservation, NativeTurnService},
};
use pontia_core::domain::EventType;
use pontia_core::{Error, Result};
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
        NativeSessionService::new(self.pool.clone(), self.event_ingest.clone()).ready(session, &runtime.instance_id, json!({"client_session_key":thread["id"],"launch_cwd":thread["cwd"],"client_session_file":thread["path"]})).await?;
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
        let items = turn["items"].as_array().cloned().unwrap_or_default();
        let input = items
            .iter()
            .find(|item| item["type"] == "userMessage")
            .and_then(|item| item["content"].as_array())
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|part| part["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        let kind = match turn["status"].as_str() {
            Some("completed") => Ok(Some(EventType::TurnCompleted)),
            Some("failed") => Ok(Some(EventType::TurnFailed)),
            Some("interrupted") => Ok(Some(EventType::TurnInterrupted)),
            Some("inProgress") => Ok(None),
            _ => Err(Error::Domain("Unknown Codex turn status".into())),
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
        NativeTurnService::new(self.pool.clone(), self.event_ingest.clone())
            .observe_turn(
                session,
                runtime_instance_id,
                NativeTurnObservation {
                    native_turn_id: native_id.into(),
                    input_summary: bounded(input.as_deref()),
                    output_summary: bounded(final_message.and_then(|item| item["text"].as_str())),
                    terminal: kind,
                    started_at: turn["startedAt"].clone(),
                    completed_at: turn["completedAt"].clone(),
                    failure: bounded(turn.pointer("/error/message").and_then(Value::as_str)),
                    origin: origin.into(),
                },
            )
            .await
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
        let target = pontia_application::runtime::ControlTarget::resolve(
            &self.pool,
            session,
            Some(&runtime.instance_id),
        )
        .await?;
        if let Some(binding) = pontia_application::AgentBindingService::new(self.pool.clone())
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
            self.bind(
                session,
                runtime,
                &snapshot["thread"],
                target.runtime_instance_id.as_deref(),
            )
            .await?;
            let connection = runtime.connection().await?;
            let turns = self.turns(&connection, &binding.client_session_key).await?;
            self.reconcile_turns(session, runtime, &turns).await?;
        }
        NativeSessionService::new(self.pool.clone(), self.event_ingest.clone())
            .exited(session, &runtime.instance_id, "thread_archived")
            .await?;
        self.connection_state(session, &runtime.instance_id, "archived")
            .await
    }
}

fn bounded(text: Option<&str>) -> Option<String> {
    text.map(|text| text.chars().take(200).collect())
}
