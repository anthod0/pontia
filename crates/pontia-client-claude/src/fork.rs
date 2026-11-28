use crate::{Adapter, Peer, peer::State, unavailable};
use pontia_agent_control::{
    EventIngestService, ReportedFact,
    client_contract::NativeSessionFork,
    sessions::{NativeSessionIdentity, NativeSubscription},
};
use pontia_core::{
    Error, Result,
    domain::{EventType, TurnState},
};
use serde_json::{Map, Value, json};
use std::sync::Arc;

struct InheritedTurn {
    turn_id: String,
    input_summary: Option<String>,
    output_summary: Option<String>,
    failure_message: Option<String>,
    state: TurnState,
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
        let starts = events
            .store()
            .queries()
            .list_events_by_type(session, EventType::TurnStarted, Some(turn))
            .await?;
        let target = starts
            .first()
            .and_then(|payload| payload["claude_input_id"].as_str())
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
        let existing = events
            .store()
            .bindings()
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
        let source = events
            .store()
            .bindings()
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
        let instructions = events
            .store()
            .instructions()
            .session_instructions(parent)
            .await?;
        let queries = events.store().queries();
        let settings = queries
            .session_launch_settings(parent)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Session {parent} not found")))?;
        let reasoning_effort = settings.reasoning_effort;
        let session = match existing {
            Some(binding) => binding.session_id,
            None => {
                events
                    .native_sessions()
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
                            launch_mode: queries.session_launch_mode(parent).await?,
                            runtime_environment: json!(settings.runtime_environment),
                            credential_env: settings.credential_env,
                        },
                        Some(child),
                    )
                    .await?
            }
        };
        events
            .native_sessions()
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
        let environment = queries.runtime_environment(&session).await?;
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
            let turn = events
                .store()
                .queries()
                .get_turn(child, &turn_id)
                .await?
                .ok_or_else(|| Error::NotFound(format!("Turn {turn_id} not found")))?;
            if matches!(turn.state.as_str(), "completed" | "failed" | "interrupted") {
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
            let terminal = match inherited.state {
                TurnState::Completed => EventType::TurnCompleted,
                TurnState::Failed => EventType::TurnFailed,
                TurnState::Interrupted => EventType::TurnInterrupted,
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
    events
        .store()
        .queries()
        .completed_turns_before_with_boundaries(session, target)
        .await?
        .into_iter()
        .map(|turn| {
            Ok(InheritedTurn {
                turn_id: turn.turn_id,
                state: turn.state,
                input_summary: turn.input_summary,
                output_summary: turn.output_summary,
                failure_message: turn.failure_message,
                head: cursor_message_id(&turn.start_cursor)?,
                tail: cursor_message_id(&turn.end_cursor)?,
            })
        })
        .collect()
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
