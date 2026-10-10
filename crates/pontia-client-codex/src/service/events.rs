use super::{CodexService, string};
use crate::runtime::CodexRuntime;
use pontia_application::turns::{NativeTurnObservation, NativeTurnService};
use pontia_core::{Error, Result, domain::EventType};
use serde_json::{Value, json};
use std::collections::HashSet;

pub(super) struct ObservedTurn {
    pub turn: Value,
    pub notification_boundary: u64,
}

impl CodexService {
    pub(super) async fn ready(
        &self,
        session: &str,
        runtime: &CodexRuntime,
        thread: &Value,
    ) -> Result<()> {
        runtime.current_guard().await?;
        if thread["canAcceptDirectInput"].as_bool() != Some(true)
            || !matches!(
                thread.pointer("/status/type").and_then(Value::as_str),
                Some("idle" | "active" | "systemError")
            )
        {
            return Err(Error::CapabilityUnavailable(
                "Codex thread is not ready to accept input".into(),
            ));
        }
        if !self.session_accepts_control(session).await? {
            self.event_ingest.report_client_fact(session, None, EventType::SessionReady,
                json!({"client_session_key":thread["id"],"launch_cwd":thread["cwd"],"client_session_file":thread["path"]})).await?;
        }
        Ok(())
    }

    pub(super) async fn turns(
        &self,
        connection: &crate::runtime::protocol::Connection,
        thread: &str,
    ) -> Result<Option<Vec<Value>>> {
        self.observed_turns(connection, thread).await.map(|turns| {
            turns.map(|turns| turns.into_iter().map(|snapshot| snapshot.turn).collect())
        })
    }

    pub(super) async fn observed_turns(
        &self,
        connection: &crate::runtime::protocol::Connection,
        thread: &str,
    ) -> Result<Option<Vec<ObservedTurn>>> {
        let mut cursor = Value::Null;
        let mut seen = HashSet::new();
        let mut turns = Vec::new();
        loop {
            let (response, sequence) = match connection
                .call_observed(
                    "thread/turns/list",
                    json!({"threadId":thread,"cursor":cursor,"limit":100,"sortDirection":"asc","itemsView":"full"}),
                )
                .await
            {
                Ok(response) => response,
                // An unmaterialized thread has not accepted its first user message yet.
                Err(Error::Conflict {
                    code: "codex_thread_not_materialized",
                    ..
                }) if cursor.is_null() => return Ok(None),
                Err(error) => return Err(error),
            };
            let page = response["data"]
                .as_array()
                .ok_or_else(|| Error::Domain("Codex turns/list has no data".into()))?;
            turns.extend(page.iter().cloned().map(|turn| ObservedTurn {
                turn,
                notification_boundary: sequence,
            }));
            cursor = response["nextCursor"].clone();
            if cursor.is_null() {
                break;
            }
            if !seen.insert(cursor.to_string()) {
                return Err(Error::Domain("Codex repeated history cursor".into()));
            }
        }
        Ok(Some(turns))
    }

    pub(super) async fn reconcile_turns(
        &self,
        session: &str,
        runtime: &CodexRuntime,
        turns: &[Value],
    ) -> Result<()> {
        let _current = runtime.current_guard().await?;
        for turn in turns {
            self.turn_fact(session, turn, "snapshot").await?;
        }
        Ok(())
    }

    pub(super) async fn turn_fact(&self, session: &str, turn: &Value, origin: &str) -> Result<()> {
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
        let terminal = match turn["status"].as_str() {
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
                None,
                NativeTurnObservation {
                    native_turn_id: native_id.into(),
                    input_summary: bounded(input.as_deref()),
                    output_summary: bounded(final_message.and_then(|item| item["text"].as_str())),
                    terminal,
                    started_at: turn["startedAt"].clone(),
                    completed_at: turn["completedAt"].clone(),
                    failure: bounded(turn.pointer("/error/message").and_then(Value::as_str)),
                    origin: origin.into(),
                },
            )
            .await
    }
}

fn bounded(text: Option<&str>) -> Option<String> {
    text.map(|text| text.chars().take(200).collect())
}
