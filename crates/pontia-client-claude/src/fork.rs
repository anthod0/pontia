use crate::{Adapter, Peer, peer::State, unavailable};
use pontia_agent_control::{
    AgentBindingService, EventIngestService, ReportedFact,
    client_contract::NativeSessionFork,
    sessions::{NativeSessionIdentity, NativeSessionService, NativeSubscription},
};
use pontia_core::{Error, Result, domain::EventType};
use serde_json::{Map, Value, json};
use std::sync::Arc;

struct InheritedTurn {
    turn_id: String,
    input_summary: Option<String>,
    output_summary: Option<String>,
    failure_message: Option<String>,
    state: String,
    head: String,
    tail: String,
}

impl Adapter {
    pub(crate) async fn fork_bound_session(
        &self,
        events: EventIngestService,
        session: &str,
        turn: &str,
    ) -> Result<NativeSessionFork> {
        let peer = self.peer(session).await?;
        let native = peer
            .state
            .lock()
            .await
            .native
            .clone()
            .ok_or_else(unavailable)?;
        let target: String = sqlx::query_scalar(
            "SELECT json_extract(payload,'$.claude_input_id') FROM events WHERE session_id=? AND turn_id=? AND event_type='turn.started' ORDER BY rowid LIMIT 1",
        )
        .bind(session)
        .bind(turn)
        .fetch_optional(&events.db())
        .await?
        .flatten()
        .ok_or_else(|| Error::StateConflict("Claude fork target is unavailable".into()))?;
        let inherited = inherited_turns(&events, session, turn).await?;
        let message_ids = inherited
            .iter()
            .flat_map(|turn| [&turn.head, &turn.tail])
            .collect::<Vec<_>>();
        let result = peer
            .connection()?
            .request(json!({
                "method": "fork",
                "target_message_id": target,
                "message_ids": message_ids,
            }))
            .await?;
        let child = result["session_id"]
            .as_str()
            .filter(|value| !value.is_empty() && *value != native)
            .ok_or_else(|| Error::ControlUnknown("Claude fork identity is unavailable".into()))?;
        let message_ids = result["message_ids"].as_object().ok_or_else(|| {
            Error::ControlUnknown("Claude fork history mapping is unavailable".into())
        })?;
        if message_ids
            .iter()
            .any(|(source, child)| source.is_empty() || !child.is_string())
        {
            return Err(Error::ControlUnknown(
                "Claude fork history mapping is invalid".into(),
            ));
        }
        Ok(NativeSessionFork {
            client_session_key: child.into(),
            receipt: json!({
                "resume": result["resume"],
                "message_ids": message_ids,
                "pontia_session_id": pontia_core::ids::new_session_id().to_string(),
                "source_turn_id": turn,
                "source_message_id": target,
            }),
        })
    }

    pub(crate) async fn adopt_bound_fork(
        &self,
        events: EventIngestService,
        parent: &str,
        fork: &NativeSessionFork,
    ) -> Result<String> {
        let child = fork.receipt["pontia_session_id"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::ControlUnknown("Claude fork receipt is incomplete".into()))?;
        let existing = AgentBindingService::new(events.db())
            .binding_for_client_session("claude-agent", &fork.client_session_key)
            .await?;
        if existing
            .as_ref()
            .is_some_and(|binding| binding.session_id != child)
        {
            return Err(Error::StateConflict(
                "Claude fork belongs to another Session".into(),
            ));
        }
        let source_turn = fork.receipt["source_turn_id"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::ControlUnknown("Claude fork receipt is incomplete".into()))?;
        let source = AgentBindingService::new(events.db())
            .binding_for_session(parent)
            .await?
            .ok_or_else(unavailable)?;
        if let Some(binding) = &existing
            && let Ok(peer) = self.peer(&binding.session_id).await
            && peer
                .connection()
                .is_ok_and(|connection| !connection.rpc.is_closed())
        {
            self.restore_inherited_turns(
                &events,
                &binding.session_id,
                &peer.runtime,
                parent,
                source_turn,
                fork,
            )
            .await?;
            return Ok(binding.session_id.clone());
        }
        let instructions = pontia_agent_control::SessionInstructionsService::new(events.db())
            .session_instructions(parent)
            .await?;
        let (environment, credential_env): (String, Option<String>) = sqlx::query_as(
            "SELECT runtime_environment,credential_env FROM sessions WHERE session_id=?",
        )
        .bind(parent)
        .fetch_one(&events.db())
        .await?;
        let reasoning_effort: Option<String> = sqlx::query_scalar(
            "SELECT json_extract(metadata,'$.reasoning_effort') FROM sessions WHERE session_id=?",
        )
        .bind(parent)
        .fetch_one(&events.db())
        .await?;
        let session = match existing {
            Some(binding) => binding.session_id,
            None => {
                NativeSessionService::new(events.db(), events.clone())
                    .resolve_subscription_session(
                        "claude-agent",
                        &fork.client_session_key,
                        NativeSessionIdentity {
                            launch_cwd: source.launch_cwd.clone(),
                            client_session_file: None,
                        },
                        NativeSubscription {
                            metadata: json!({"reasoning_effort": reasoning_effort}),
                            binding_metadata: source.metadata.clone(),
                            session_instructions: instructions,
                            launch_mode: pontia_agent_control::ControlQueryService::new(
                                events.db(),
                            )
                            .session_launch_mode(parent)
                            .await?,
                            runtime_environment: serde_json::from_str(&environment)?,
                            credential_env,
                        },
                        Some(child),
                    )
                    .await?
            }
        };
        NativeSessionService::new(events.db(), events.clone())
            .record_fork(
                &session,
                parent,
                Some(source_turn),
                fork.receipt["source_message_id"].as_str(),
                json!({"boundary":"before"}),
            )
            .await?;

        let runtime = pontia_core::ids::new_runtime_id().to_string();
        let resume = fork.receipt["resume"] == true;
        let peer = Arc::new(Peer {
            session: session.clone(),
            runtime: runtime.clone(),
            cwd: source.launch_cwd.into(),
            config_dir: source.metadata["claude_config_dir"]
                .as_str()
                .ok_or_else(unavailable)?
                .into(),
            model: source.metadata["launch_model"].as_str().map(str::to_owned),
            resume: resume.then(|| fork.client_session_key.clone()),
            native_session_id: Some(fork.client_session_key.clone()),
            interactions: self.interactions.get().cloned().ok_or_else(unavailable)?,
            state: tokio::sync::Mutex::new(State {
                native: Some(fork.client_session_key.clone()),
                ..State::default()
            }),
            connection: Default::default(),
        });
        self.peers
            .lock()
            .await
            .insert(session.clone(), peer.clone());
        let (_, root) = self.context.get().ok_or_else(unavailable)?;
        let script = self.script(root)?;
        let environment = pontia_agent_control::ControlQueryService::new(events.db())
            .runtime_environment(&session)
            .await?;
        peer.start(
            &events,
            root,
            &script,
            &environment,
            reasoning_effort.as_deref(),
        )
        .await?;
        self.restore_inherited_turns(&events, &session, &runtime, parent, source_turn, fork)
            .await?;
        Ok(session)
    }

    async fn restore_inherited_turns(
        &self,
        events: &EventIngestService,
        child: &str,
        runtime: &str,
        parent: &str,
        source_turn: &str,
        fork: &NativeSessionFork,
    ) -> Result<()> {
        let mappings = fork.receipt["message_ids"]
            .as_object()
            .ok_or_else(|| Error::ControlUnknown("Claude fork receipt is incomplete".into()))?;
        for inherited in inherited_turns(events, parent, source_turn).await? {
            let (Some(head), Some(tail)) = (
                mapped_message(mappings, &inherited.head),
                mapped_message(mappings, &inherited.tail),
            ) else {
                continue;
            };
            let started = events
                .report_fact(ReportedFact {
                    session_id: child.into(),
                    turn_id: None,
                    report_key: Some(format!(
                        "claude-fork:{}:{}:started",
                        fork.client_session_key, inherited.turn_id
                    )),
                    fact_type: EventType::TurnStarted,
                    data: json!({
                        "runtime_id": runtime,
                        "claude_history_anchor": head,
                        "input": {"summary": inherited.input_summary},
                        "metadata": {"observation":"fork"},
                    }),
                })
                .await
                .map_err(report_error)?;
            let turn_id = started
                .turn_id
                .ok_or_else(|| Error::Domain("Forked Claude Turn identity is missing".into()))?;
            let state: String =
                sqlx::query_scalar("SELECT state FROM turns WHERE turn_id=? AND session_id=?")
                    .bind(&turn_id)
                    .bind(child)
                    .fetch_one(&events.db())
                    .await?;
            if matches!(state.as_str(), "completed" | "failed" | "interrupted") {
                continue;
            }
            if let Some(summary) = inherited.output_summary {
                events
                    .report_fact(ReportedFact {
                        session_id: child.into(),
                        turn_id: Some(turn_id.clone()),
                        report_key: None,
                        fact_type: EventType::TurnOutput,
                        data: json!({"runtime_id":runtime,"output":{"summary":summary}}),
                    })
                    .await
                    .map_err(report_error)?;
            }
            let terminal = match inherited.state.as_str() {
                "completed" => EventType::TurnCompleted,
                "failed" => EventType::TurnFailed,
                "interrupted" => EventType::TurnInterrupted,
                _ => continue,
            };
            events
                .report_fact(ReportedFact {
                    session_id: child.into(),
                    turn_id: Some(turn_id),
                    report_key: Some(format!(
                        "claude-fork:{}:{}:finished",
                        fork.client_session_key, inherited.turn_id
                    )),
                    fact_type: terminal,
                    data: json!({
                        "runtime_id": runtime,
                        "claude_history_anchor": tail,
                        "failure": {"message": inherited.failure_message},
                        "observation":"fork",
                    }),
                })
                .await
                .map_err(report_error)?;
        }
        Ok(())
    }
}

fn mapped_message<'a>(mappings: &'a Map<String, Value>, source: &str) -> Option<&'a str> {
    mappings
        .get(source)?
        .as_str()
        .filter(|value| !value.is_empty())
}

fn report_error(error: pontia_agent_control::EventReportError) -> Error {
    match error {
        pontia_agent_control::EventReportError::InvalidFact(message) => Error::Domain(message),
        pontia_agent_control::EventReportError::Ingestion(error) => error,
    }
}

async fn inherited_turns(
    events: &EventIngestService,
    session: &str,
    target: &str,
) -> Result<Vec<InheritedTurn>> {
    type InheritedTurnRow = (
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
    );
    let rows: Vec<InheritedTurnRow> = sqlx::query_as(
            "SELECT t.turn_id,t.input_summary,t.output_summary,
                    json_extract((SELECT payload FROM events e WHERE e.session_id=t.session_id AND e.turn_id=t.turn_id AND e.event_type IN ('turn.completed','turn.failed','turn.interrupted') ORDER BY rowid DESC LIMIT 1),'$.failure.message'),t.state,
                    json_extract((SELECT timeline_boundary FROM events e WHERE e.session_id=t.session_id AND e.turn_id=t.turn_id AND e.event_type='turn.started' ORDER BY rowid LIMIT 1),'$.cursor'),
                    json_extract((SELECT timeline_boundary FROM events e WHERE e.session_id=t.session_id AND e.turn_id=t.turn_id AND e.event_type IN ('turn.completed','turn.failed','turn.interrupted') ORDER BY rowid DESC LIMIT 1),'$.cursor')
             FROM turns t JOIN turns target ON target.session_id=t.session_id AND target.turn_id=?
             WHERE t.session_id=? AND t.rowid<target.rowid AND t.state IN ('completed','failed','interrupted')
               AND EXISTS(SELECT 1 FROM events e WHERE e.session_id=t.session_id AND e.turn_id=t.turn_id AND e.event_type='turn.started' AND e.timeline_boundary IS NOT NULL)
               AND EXISTS(SELECT 1 FROM events e WHERE e.session_id=t.session_id AND e.turn_id=t.turn_id AND e.event_type IN ('turn.completed','turn.failed','turn.interrupted') AND e.timeline_boundary IS NOT NULL)
             ORDER BY t.rowid",
        )
        .bind(target)
        .bind(session)
        .fetch_all(&events.db())
        .await?;
    rows.into_iter()
        .map(
            |(turn_id, input_summary, output_summary, failure_message, state, head, tail)| {
                Ok(InheritedTurn {
                    turn_id,
                    input_summary,
                    output_summary,
                    failure_message,
                    state,
                    head: cursor_message_id(&head)?,
                    tail: cursor_message_id(&tail)?,
                })
            },
        )
        .collect::<Result<Vec<_>>>()
}

fn cursor_message_id(cursor: &str) -> Result<String> {
    let cursor: Value = serde_json::from_str(cursor)?;
    cursor
        .get(3)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| Error::StateConflict("Claude Turn history boundary is unavailable".into()))
}
